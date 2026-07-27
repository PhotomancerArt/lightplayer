//! [`AgentSession`]: the agentic loop.
//!
//! Drives one user message to completion: model turn → execute tool calls
//! via the injected [`AgentHost`] → append results → repeat until
//! `end_turn`, the turn limit, or abort. Runtime-neutral async: no spawning,
//! no sleeps — whoever awaits `run` drives everything (Studio wasm main
//! thread, `block_on` in tests/evals).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use lps_probe::CompiledShader;
use serde_json::json;

use crate::prompt::system_prompt::build_system_prompt;
use crate::provider::model_provider::{
    ContentBlock, ModelProvider, StopReason, TurnEvent, TurnRequest,
};
use crate::session::agent_event::AgentEvent;
use crate::session::agent_transcript::AgentTranscript;
use crate::tool::iterate_host::AgentHost;
use crate::tool::iterate_tool::{ITERATE_TOOL_NAME, IterateOutcome, iterate_tool_def, run_iterate};

/// Model turns (API calls) allowed per user message.
pub const MAX_TURNS_PER_RUN: u32 = 16;
/// Default per-turn output token budget.
pub const DEFAULT_MAX_TOKENS: u32 = 8192;

/// A session run failed (the transcript keeps everything up to the failure).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentError {
    /// The provider errored or its stream ended without completing the turn.
    Provider { message: String },
}

/// One agent conversation over one shader.
pub struct AgentSession<P: ModelProvider, H: AgentHost> {
    provider: P,
    host: H,
    transcript: AgentTranscript,
    /// Per-turn output token budget.
    pub max_tokens: u32,
    /// Model turns allowed per `run` call.
    pub max_turns: u32,
    abort: Arc<AtomicBool>,
    /// Last successfully compiled shader (the `iterate` diff cache).
    prev_compiled: Option<CompiledShader>,
}

impl<P: ModelProvider, H: AgentHost> AgentSession<P, H> {
    pub fn new(provider: P, host: H) -> Self {
        Self {
            provider,
            host,
            transcript: AgentTranscript::default(),
            max_tokens: DEFAULT_MAX_TOKENS,
            max_turns: MAX_TURNS_PER_RUN,
            abort: Arc::new(AtomicBool::new(false)),
            prev_compiled: None,
        }
    }

    /// Shared flag the UI's Stop button flips; checked between events and
    /// tool executions (dropping the turn stream cancels the transport).
    pub fn abort_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.abort)
    }

    /// Replace the provider while keeping the transcript and diff cache —
    /// the embedder's settings (API key, model) may change between runs.
    pub fn set_provider(&mut self, provider: P) {
        self.provider = provider;
    }

    pub fn transcript(&self) -> &AgentTranscript {
        &self.transcript
    }

    /// Drive one user message to completion, emitting [`AgentEvent`]s.
    pub async fn run(
        &mut self,
        user_msg: String,
        mut on_event: impl FnMut(AgentEvent),
    ) -> Result<(), AgentError> {
        self.abort.store(false, Ordering::Relaxed);
        self.transcript.push_user_text(user_msg);

        for _turn in 0..self.max_turns {
            let step = self.run_one_turn(&mut on_event).await?;
            match step {
                TurnStep::Continue => {}
                TurnStep::Done => {
                    on_event(AgentEvent::SessionDone {
                        usage_total: self.transcript.usage_total,
                    });
                    return Ok(());
                }
                TurnStep::Aborted => {
                    on_event(AgentEvent::Aborted);
                    on_event(AgentEvent::SessionDone {
                        usage_total: self.transcript.usage_total,
                    });
                    return Ok(());
                }
            }
        }

        // Turn limit: nudge the transcript so the next run starts from an
        // honest state, and stop cleanly.
        self.transcript.push_user_text(format!(
            "[system] Turn limit reached ({} model turns for one request). Stop \
             iterating, summarize progress and open questions for the user, and \
             wait for their next instruction.",
            self.max_turns
        ));
        on_event(AgentEvent::MaxTurnsReached {
            turns: self.max_turns,
        });
        on_event(AgentEvent::SessionDone {
            usage_total: self.transcript.usage_total,
        });
        Ok(())
    }

    /// One model turn plus its tool executions.
    async fn run_one_turn(
        &mut self,
        on_event: &mut impl FnMut(AgentEvent),
    ) -> Result<TurnStep, AgentError> {
        // Rebuilt every turn: staged edits change the current source.
        let current_source = self.host.current_source().unwrap_or_default();
        let system = build_system_prompt(&self.host.shader_context(), &current_source);
        let req = TurnRequest {
            system,
            messages: self.transcript.messages.clone(),
            tools: vec![iterate_tool_def()],
            max_tokens: self.max_tokens,
        };

        let mut blocks: Vec<Acc> = Vec::new();
        let mut outcome: Option<StopReason> = None;
        {
            let mut stream = self.provider.run_turn(req);
            while let Some(ev) = stream.next().await {
                if self.abort.load(Ordering::Relaxed) {
                    return Ok(TurnStep::Aborted); // dropping `stream` cancels
                }
                match ev {
                    TurnEvent::TextDelta(text) => {
                        match blocks.last_mut() {
                            Some(Acc::Text(t)) => t.push_str(&text),
                            _ => blocks.push(Acc::Text(text.clone())),
                        }
                        on_event(AgentEvent::TextDelta(text));
                    }
                    TurnEvent::ToolUseStart { id, name } => {
                        blocks.push(Acc::Tool {
                            id: id.clone(),
                            name: name.clone(),
                            input_json: String::new(),
                        });
                        on_event(AgentEvent::ToolUseStart { id, name });
                    }
                    TurnEvent::ToolInputDelta { id, json_fragment } => {
                        if let Some(Acc::Tool { input_json, .. }) =
                            blocks.iter_mut().rev().find(|b| b.tool_id() == Some(&id))
                        {
                            input_json.push_str(&json_fragment);
                        }
                        on_event(AgentEvent::ToolInputDelta { id, json_fragment });
                    }
                    TurnEvent::TurnDone { stop_reason, usage } => {
                        self.transcript.usage_total.add(usage);
                        on_event(AgentEvent::TurnDone {
                            stop_reason: stop_reason.clone(),
                            usage,
                        });
                        outcome = Some(stop_reason);
                        break;
                    }
                    TurnEvent::ProviderError { message, retryable } => {
                        on_event(AgentEvent::ProviderError {
                            message: message.clone(),
                            retryable,
                        });
                        return Err(AgentError::Provider { message });
                    }
                }
            }
        }
        let Some(stop_reason) = outcome else {
            let message = "turn stream ended without completing".to_string();
            on_event(AgentEvent::ProviderError {
                message: message.clone(),
                retryable: true,
            });
            return Err(AgentError::Provider { message });
        };

        // Record the assistant message (parse accumulated tool input JSON
        // now, at end of turn).
        let content: Vec<ContentBlock> = blocks.iter().map(Acc::to_content_block).collect();
        let tool_calls: Vec<(String, String, serde_json::Value)> = content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();
        self.transcript.push_assistant(content);

        if stop_reason != StopReason::ToolUse || tool_calls.is_empty() {
            return Ok(TurnStep::Done);
        }

        let mut results = Vec::new();
        for (id, name, input) in tool_calls {
            if self.abort.load(Ordering::Relaxed) {
                self.transcript.push_tool_results(results);
                return Ok(TurnStep::Aborted);
            }
            let outcome = if name == ITERATE_TOOL_NAME {
                run_iterate(&input, &mut self.host, &mut self.prev_compiled)
            } else {
                IterateOutcome {
                    content: json!({ "error": format!("unknown tool {name:?}") }).to_string(),
                    is_error: true,
                    summary: json!({ "error": "unknown tool" }),
                }
            };
            on_event(AgentEvent::ToolExecuted {
                id: id.clone(),
                name,
                summary_json: outcome.summary,
            });
            results.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: outcome.content,
                is_error: outcome.is_error.then_some(true),
            });
        }
        self.transcript.push_tool_results(results);
        Ok(TurnStep::Continue)
    }
}

/// How one turn left the loop.
enum TurnStep {
    Continue,
    Done,
    Aborted,
}

/// Accumulator for one streamed content block.
enum Acc {
    Text(String),
    Tool {
        id: String,
        name: String,
        input_json: String,
    },
}

impl Acc {
    fn tool_id(&self) -> Option<&String> {
        match self {
            Acc::Tool { id, .. } => Some(id),
            Acc::Text(_) => None,
        }
    }

    fn to_content_block(&self) -> ContentBlock {
        match self {
            Acc::Text(text) => ContentBlock::Text { text: text.clone() },
            Acc::Tool {
                id,
                name,
                input_json,
            } => {
                let raw = input_json.trim();
                // A tool call with no streamed input means `{}`.
                let input = if raw.is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(raw).unwrap_or_else(|_| {
                        // Malformed input JSON: preserve it verbatim so the
                        // tool layer reports it in-band.
                        serde_json::Value::String(input_json.clone())
                    })
                };
                ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use futures_util::stream;
    use lps_probe::LedPoint;

    use super::*;
    use crate::provider::model_provider::{BoxStream, ChatRole, TokenUsage};
    use crate::tool::iterate_host::{HostError, ShaderContext};

    const RED: &str = "vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }";
    const GREEN: &str = "vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.0, 1.0); }";

    #[test]
    fn text_only_turn_completes_the_session() {
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::TextDelta("All ".into()),
            TurnEvent::TextDelta("done.".into()),
            turn_done(StopReason::EndTurn, 10, 5),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("hello".into(), |e| events.push(e))).expect("run");

        assert!(
            matches!(events.last(), Some(AgentEvent::SessionDone { usage_total })
            if usage_total.input_tokens == 10 && usage_total.output_tokens == 5)
        );
        let messages = &session.transcript().messages;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(
            messages[1].content,
            vec![ContentBlock::Text {
                text: "All done.".into()
            }]
        );
        // The system prompt was built from the host context each turn.
        let reqs = provider.requests.borrow();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].system.contains(RED));
        assert_eq!(reqs[0].tools.len(), 1);
        assert_eq!(reqs[0].tools[0].name, "iterate");
    }

    #[test]
    fn tool_turn_stages_source_and_feeds_result_back() {
        let input = format!("{{\"source\": {}, \"note\": \"go green\"}}", json!(GREEN));
        let mid = input.len() / 2;
        let provider = FakeProvider::new(vec![
            vec![
                TurnEvent::TextDelta("Trying green.".into()),
                TurnEvent::ToolUseStart {
                    id: "tu_1".into(),
                    name: "iterate".into(),
                },
                TurnEvent::ToolInputDelta {
                    id: "tu_1".into(),
                    json_fragment: input[..mid].into(),
                },
                TurnEvent::ToolInputDelta {
                    id: "tu_1".into(),
                    json_fragment: input[mid..].into(),
                },
                turn_done(StopReason::ToolUse, 20, 30),
            ],
            vec![
                TurnEvent::TextDelta("It is green now.".into()),
                turn_done(StopReason::EndTurn, 40, 10),
            ],
        ]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("make it green".into(), |e| events.push(e))).expect("run");

        // The host saw the staged edit.
        assert_eq!(session.host.staged.borrow().as_slice(), [GREEN.to_string()]);
        // ToolExecuted carried the compact summary.
        let executed = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::ToolExecuted {
                    id, summary_json, ..
                } => Some((id.clone(), summary_json.clone())),
                _ => None,
            })
            .expect("ToolExecuted");
        assert_eq!(executed.0, "tu_1");
        assert_eq!(executed.1["shader_ok"], true);
        assert_eq!(executed.1["note"], "go green");

        // Transcript: user, assistant(text+tool_use), user(tool_result),
        // assistant(text).
        let messages = &session.transcript().messages;
        assert_eq!(messages.len(), 4);
        assert!(matches!(
            &messages[1].content[1],
            ContentBlock::ToolUse { name, input, .. }
                if name == "iterate" && input["note"] == "go green"
        ));
        match &messages[2].content[0] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tu_1");
                assert!(content.contains("\"shader\":\"ok\""), "{content}");
                assert_eq!(*is_error, None);
            }
            other => panic!("unexpected: {other:?}"),
        }
        // Usage accumulated across both turns.
        assert_eq!(
            session.transcript().usage_total,
            TokenUsage {
                input_tokens: 60,
                output_tokens: 40,
                ..TokenUsage::default()
            }
        );
        // The second turn's system prompt reflects the staged source.
        assert!(provider.requests.borrow()[1].system.contains(GREEN));
    }

    #[test]
    fn max_turns_stops_cleanly_with_a_nudge() {
        let tool_turn = || {
            vec![
                TurnEvent::ToolUseStart {
                    id: "tu".into(),
                    name: "iterate".into(),
                },
                turn_done(StopReason::ToolUse, 1, 1),
            ]
        };
        let provider = FakeProvider::new(vec![tool_turn(), tool_turn(), tool_turn()]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        session.max_turns = 2;
        let mut events = Vec::new();
        block_on(session.run("loop forever".into(), |e| events.push(e))).expect("run");

        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::MaxTurnsReached { turns: 2 }))
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::SessionDone { .. })
        ));
        assert_eq!(provider.requests.borrow().len(), 2);
        let last = session.transcript().messages.last().expect("nudge");
        assert!(matches!(&last.content[0], ContentBlock::Text { text }
            if text.contains("Turn limit reached")));
    }

    #[test]
    fn abort_between_events_stops_the_run() {
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::TextDelta("one".into()),
            TurnEvent::TextDelta("two".into()),
            turn_done(StopReason::EndTurn, 1, 1),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let abort = session.abort_handle();
        let mut events = Vec::new();
        block_on(session.run("go".into(), |e| {
            // Flip the stop flag from inside the first event, like the UI's
            // Stop button would mid-stream.
            abort.store(true, Ordering::Relaxed);
            events.push(e);
        }))
        .expect("run");
        assert_eq!(
            events,
            vec![
                AgentEvent::TextDelta("one".into()),
                AgentEvent::Aborted,
                AgentEvent::SessionDone {
                    usage_total: TokenUsage::default()
                }
            ]
        );
    }

    #[test]
    fn provider_error_fails_the_run() {
        let provider = FakeProvider::new(vec![vec![TurnEvent::ProviderError {
            message: "boom".into(),
            retryable: false,
        }]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        let err = block_on(session.run("go".into(), |e| events.push(e))).expect_err("fails");
        assert_eq!(
            err,
            AgentError::Provider {
                message: "boom".into()
            }
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::ProviderError { .. })
        ));
    }

    #[test]
    fn unknown_tool_returns_error_result_and_continues() {
        let provider = FakeProvider::new(vec![
            vec![
                TurnEvent::ToolUseStart {
                    id: "tu_1".into(),
                    name: "teleport".into(),
                },
                turn_done(StopReason::ToolUse, 1, 1),
            ],
            vec![
                TurnEvent::TextDelta("ok".into()),
                turn_done(StopReason::EndTurn, 1, 1),
            ],
        ]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("go".into(), |e| events.push(e))).expect("run");
        match &session.transcript().messages[2].content[0] {
            ContentBlock::ToolResult {
                content, is_error, ..
            } => {
                assert!(content.contains("unknown tool"), "{content}");
                assert_eq!(*is_error, Some(true));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    // -- helpers ----------------------------------------------------------

    fn turn_done(stop_reason: StopReason, input: u32, output: u32) -> TurnEvent {
        TurnEvent::TurnDone {
            stop_reason,
            usage: TokenUsage {
                input_tokens: input,
                output_tokens: output,
                ..TokenUsage::default()
            },
        }
    }

    fn block_on<F: Future>(f: F) -> F::Output {
        futures_executor::block_on(f)
    }

    struct FakeProvider {
        turns: RefCell<VecDeque<Vec<TurnEvent>>>,
        requests: RefCell<Vec<TurnRequest>>,
    }

    impl FakeProvider {
        fn new(turns: Vec<Vec<TurnEvent>>) -> Self {
            Self {
                turns: RefCell::new(turns.into()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl ModelProvider for &FakeProvider {
        fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
            self.requests.borrow_mut().push(req);
            let events = self.turns.borrow_mut().pop_front().unwrap_or_default();
            Box::pin(stream::iter(events))
        }
    }

    struct FakeHost {
        source: RefCell<String>,
        staged: RefCell<Vec<String>>,
    }

    impl FakeHost {
        fn new(source: &str) -> Self {
            Self {
                source: RefCell::new(source.to_string()),
                staged: RefCell::new(Vec::new()),
            }
        }
    }

    impl AgentHost for FakeHost {
        fn current_source(&self) -> Result<String, HostError> {
            Ok(self.source.borrow().clone())
        }

        fn stage_source(&mut self, source: &str) -> Result<(), HostError> {
            self.staged.borrow_mut().push(source.to_string());
            *self.source.borrow_mut() = source.to_string();
            Ok(())
        }

        fn led_points(&self) -> Vec<LedPoint> {
            Vec::new()
        }

        fn shader_context(&self) -> ShaderContext {
            ShaderContext {
                project_name: "Test Project".into(),
                node_name: "test-shader".into(),
                fixture: None,
                bindings: Vec::new(),
            }
        }
    }
}
