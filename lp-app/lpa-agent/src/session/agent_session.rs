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
use crate::tool::upsert_param_tool::{
    UPSERT_PARAM_TOOL_NAME, run_upsert_param, upsert_param_tool_def,
};

/// Model turns (API calls) allowed per user message.
pub const MAX_TURNS_PER_RUN: u32 = 16;

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
            tools: vec![iterate_tool_def(), upsert_param_tool_def()],
        };

        let mut blocks: Vec<Acc> = Vec::new();
        let mut outcome: Option<StopReason> = None;
        // True while a thinking segment is streaming; the transition to any
        // visible content (or the turn's end) emits the collapse boundary.
        let mut thinking_open = false;
        {
            let mut stream = self.provider.run_turn(req);
            while let Some(ev) = stream.next().await {
                if self.abort.load(Ordering::Relaxed) {
                    return Ok(TurnStep::Aborted); // dropping `stream` cancels
                }
                match ev {
                    TurnEvent::ThinkingDelta(text) => {
                        // A delta after the previous block's signature
                        // closed it starts a NEW thinking block
                        // (interleaved thinking).
                        match blocks.last_mut() {
                            Some(Acc::Thinking {
                                text: existing,
                                signature,
                            }) if signature.is_empty() => existing.push_str(&text),
                            _ => blocks.push(Acc::Thinking {
                                text: text.clone(),
                                signature: String::new(),
                            }),
                        }
                        thinking_open = true;
                        on_event(AgentEvent::ThinkingDelta(text));
                    }
                    TurnEvent::ThinkingSignature(fragment) => {
                        // Signatures accumulate onto the open block; one
                        // arriving with no block (display-omitted servers)
                        // still records a replayable empty-text block.
                        match blocks.last_mut() {
                            Some(Acc::Thinking { signature, .. }) => {
                                signature.push_str(&fragment);
                            }
                            _ => blocks.push(Acc::Thinking {
                                text: String::new(),
                                signature: fragment,
                            }),
                        }
                    }
                    TurnEvent::RedactedThinking(data) => {
                        // Opaque: transcript-only, never shown.
                        blocks.push(Acc::Redacted { data });
                    }
                    TurnEvent::TextDelta(text) => {
                        if std::mem::take(&mut thinking_open) {
                            on_event(AgentEvent::ThinkingDone);
                        }
                        match blocks.last_mut() {
                            Some(Acc::Text(t)) => t.push_str(&text),
                            _ => blocks.push(Acc::Text(text.clone())),
                        }
                        on_event(AgentEvent::TextDelta(text));
                    }
                    TurnEvent::ToolUseStart { id, name } => {
                        if std::mem::take(&mut thinking_open) {
                            on_event(AgentEvent::ThinkingDone);
                        }
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
                        if std::mem::take(&mut thinking_open) {
                            on_event(AgentEvent::ThinkingDone);
                        }
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
        let mut content: Vec<ContentBlock> = blocks.iter().map(Acc::to_content_block).collect();

        // Tool input JSON that does not parse is direct evidence the turn
        // was cut mid-call, and it outranks the server's self-report: some
        // compat servers label an output-cap cut `tool_calls` rather than
        // `length`, which would otherwise walk a torn call straight into
        // the execution path below and fail deep in the tool layer with a
        // deserialization error that names neither the cause nor the fix.
        // Note this reclassifies AFTER `TurnDone`, so the UI's usage row
        // still reflects what the provider actually reported.
        let stop_reason = if blocks.iter().any(Acc::has_malformed_tool_input) {
            StopReason::MaxTokens
        } else {
            stop_reason
        };

        if stop_reason != StopReason::ToolUse {
            // Any accumulated tool call will never execute — most often a
            // `MaxTokens` cut mid-input-JSON. Drop such blocks from the
            // recorded message instead of replaying them: the Anthropic
            // protocol requires every `tool_use` to be answered by a
            // `tool_result`, and a truncated input is garbage anyway. The
            // `Truncated` event tells the UI the run ended incomplete.
            let dropped_tool_call = content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
            content.retain(|b| !matches!(b, ContentBlock::ToolUse { .. }));
            self.transcript.push_assistant(content);
            if matches!(stop_reason, StopReason::MaxTokens | StopReason::Other(_)) {
                on_event(AgentEvent::Truncated {
                    stop_reason,
                    dropped_tool_call,
                });
            }
            return Ok(TurnStep::Done);
        }

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
        if tool_calls.is_empty() {
            return Ok(TurnStep::Done);
        }

        let mut results = Vec::new();
        let mut calls = tool_calls.into_iter();
        while let Some((id, name, input)) = calls.next() {
            if self.abort.load(Ordering::Relaxed) {
                // The recorded assistant message already carries every
                // tool_use block; the protocol still requires each to be
                // answered before the next model turn, so the unexecuted
                // remainder gets synthesized cancellation results.
                results.push(cancelled_tool_result(id));
                results.extend(calls.map(|(id, _, _)| cancelled_tool_result(id)));
                self.transcript.push_tool_results(results);
                return Ok(TurnStep::Aborted);
            }
            // The input JSON is fully accumulated here (parsed at end of
            // turn): surface the model's one-line note so the running row
            // reads "{note} — running" instead of a generic label.
            on_event(AgentEvent::ToolInputReady {
                id: id.clone(),
                note: input["note"].as_str().map(str::to_string),
            });
            let outcome = if name == ITERATE_TOOL_NAME {
                let progress_id = id.clone();
                run_iterate(
                    &input,
                    &mut self.host,
                    &mut self.prev_compiled,
                    &mut |phase| {
                        on_event(AgentEvent::ToolProgress {
                            id: progress_id.clone(),
                            phase,
                        });
                    },
                )
                .await
            } else if name == UPSERT_PARAM_TOOL_NAME {
                let progress_id = id.clone();
                run_upsert_param(&input, &mut self.host, &mut |phase| {
                    on_event(AgentEvent::ToolProgress {
                        id: progress_id.clone(),
                        phase,
                    });
                })
                .await
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

/// The synthesized result for a recorded tool call the run never executed
/// (Stop mid-turn): keeps the transcript protocol-valid — every `tool_use`
/// answered — and tells the model plainly what happened.
fn cancelled_tool_result(id: String) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id,
        content: json!({ "cancelled": "the user stopped the run before this call executed" })
            .to_string(),
        is_error: Some(true),
    }
}

/// Accumulator for one streamed content block.
enum Acc {
    Text(String),
    /// A thinking block; the signature closes it (Anthropic) or stays
    /// empty (compat reasoning — recorded for the UI mirror, never
    /// replayed to a provider).
    Thinking {
        text: String,
        signature: String,
    },
    /// An opaque redacted thinking block, passed through verbatim.
    Redacted {
        data: String,
    },
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
            Acc::Text(_) | Acc::Thinking { .. } | Acc::Redacted { .. } => None,
        }
    }

    /// Whether this is a tool call whose accumulated input JSON does not
    /// parse — a call that can never execute. The overwhelmingly common
    /// cause is the per-turn output cap landing mid-JSON; a genuinely
    /// malformed emission is indistinguishable here and is treated the
    /// same way, since neither can be run.
    fn has_malformed_tool_input(&self) -> bool {
        match self {
            Acc::Tool { input_json, .. } => {
                let raw = input_json.trim();
                !raw.is_empty() && serde_json::from_str::<serde_json::Value>(raw).is_err()
            }
            Acc::Text(_) | Acc::Thinking { .. } | Acc::Redacted { .. } => false,
        }
    }

    fn to_content_block(&self) -> ContentBlock {
        match self {
            Acc::Text(text) => ContentBlock::Text { text: text.clone() },
            Acc::Thinking { text, signature } => ContentBlock::Thinking {
                thinking: text.clone(),
                signature: signature.clone(),
            },
            Acc::Redacted { data } => ContentBlock::RedactedThinking { data: data.clone() },
            Acc::Tool {
                id,
                name,
                input_json,
            } => {
                let raw = input_json.trim();
                // A tool call with no streamed input means `{}`. Input that
                // does not parse means the call was cut mid-JSON: `run_turn`
                // detects that via `has_malformed_tool_input` and drops the
                // block before execution, so this placeholder is never read.
                // (It must not be the raw string — handing that to a tool's
                // deserializer reports a type mismatch instead of the cut.)
                let input = if raw.is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
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
    use crate::tool::iterate_host::{HostError, HostFuture, ShaderContext};
    use crate::tool::tool_phase::ToolPhase;

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
        assert_eq!(reqs[0].tools.len(), 2);
        assert_eq!(reqs[0].tools[0].name, "iterate");
        assert_eq!(reqs[0].tools[1].name, "upsert_param");
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
        // The accumulated input surfaced its note (pre-execution), then the
        // live phases, then the executed summary — in that order.
        let ready_at = events
            .iter()
            .position(|e| {
                matches!(e, AgentEvent::ToolInputReady { id, note }
                    if id == "tu_1" && note.as_deref() == Some("go green"))
            })
            .expect("ToolInputReady");
        let first_progress = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    AgentEvent::ToolProgress {
                        phase: ToolPhase::Staging,
                        ..
                    }
                )
            })
            .expect("ToolProgress(Staging)");
        let executed_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::ToolExecuted { .. }))
            .expect("ToolExecuted");
        assert!(ready_at < first_progress && first_progress < executed_at);
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
    fn thinking_blocks_round_trip_into_the_next_request() {
        // Turn 1 thinks (with a split signature), calls the tool, and the
        // session must replay the thinking block VERBATIM in turn 2's
        // request — the Anthropic tool-use protocol requirement.
        let provider = FakeProvider::new(vec![
            vec![
                TurnEvent::ThinkingDelta("Green means ".into()),
                TurnEvent::ThinkingDelta("more G.".into()),
                TurnEvent::ThinkingSignature("EqQB".into()),
                TurnEvent::ThinkingSignature("sig".into()),
                TurnEvent::RedactedThinking("opaque-data".into()),
                TurnEvent::TextDelta("Trying green.".into()),
                TurnEvent::ToolUseStart {
                    id: "tu_1".into(),
                    name: "iterate".into(),
                },
                turn_done(StopReason::ToolUse, 20, 30),
            ],
            vec![
                TurnEvent::TextDelta("Done.".into()),
                turn_done(StopReason::EndTurn, 40, 10),
            ],
        ]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("make it green".into(), |e| events.push(e))).expect("run");

        // Streamed thinking surfaced, and the boundary landed before the
        // first visible text.
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ThinkingDelta(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, ["Green means ", "more G."]);
        let done_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::ThinkingDone))
            .expect("ThinkingDone");
        let text_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::TextDelta(_)))
            .expect("TextDelta");
        assert!(done_at < text_at);

        // The assistant message carries thinking + redacted + text + tool.
        let assistant = &session.transcript().messages[1];
        assert_eq!(
            assistant.content[0],
            ContentBlock::Thinking {
                thinking: "Green means more G.".into(),
                signature: "EqQBsig".into(),
            }
        );
        assert_eq!(
            assistant.content[1],
            ContentBlock::RedactedThinking {
                data: "opaque-data".into(),
            }
        );

        // And turn 2's REQUEST replays both blocks exactly as received.
        let reqs = provider.requests.borrow();
        let replayed = &reqs[1].messages[1];
        assert_eq!(replayed.content[0], assistant.content[0]);
        assert_eq!(replayed.content[1], assistant.content[1]);
    }

    #[test]
    fn reasoning_only_turns_close_thinking_at_turn_done() {
        // Compat servers can stream reasoning with no signature; the
        // boundary must still arrive when the turn ends.
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::ThinkingDelta("pondering".into()),
            turn_done(StopReason::EndTurn, 1, 1),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("go".into(), |e| events.push(e))).expect("run");
        let done_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::ThinkingDone))
            .expect("ThinkingDone");
        let turn_done_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::TurnDone { .. }))
            .expect("TurnDone");
        assert!(done_at < turn_done_at);
        // The unsigned block is recorded (UI mirror parity)...
        assert_eq!(
            session.transcript().messages[1].content[0],
            ContentBlock::Thinking {
                thinking: "pondering".into(),
                signature: String::new(),
            }
        );
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

    #[test]
    fn max_tokens_cut_drops_the_dangling_tool_call_and_reports_truncation() {
        // The output budget ran out mid-tool-input-JSON — the live failure:
        // stream ends with `max_tokens`, the tool call's input is torn.
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::TextDelta("Rewriting the shader.".into()),
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: "{\"source\": \"vec4 render(".into(),
            },
            turn_done(StopReason::MaxTokens, 10, 8192),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("bounce a ball".into(), |e| events.push(e))).expect("run");

        // The run ends CLEANLY but loudly: Truncated then SessionDone.
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::Truncated {
                stop_reason: StopReason::MaxTokens,
                dropped_tool_call: true,
            }
        )));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::SessionDone { .. })
        ));
        // The dangling tool_use never reaches the transcript (a replayed
        // tool_use without a tool_result is a protocol violation), while
        // the text that did stream is kept.
        let assistant = &session.transcript().messages[1];
        assert_eq!(
            assistant.content,
            vec![ContentBlock::Text {
                text: "Rewriting the shader.".into()
            }]
        );
        // A follow-up run replays a valid transcript and completes.
        provider.turns.borrow_mut().push_back(vec![
            TurnEvent::TextDelta("Shorter this time.".into()),
            turn_done(StopReason::EndTurn, 1, 1),
        ]);
        block_on(session.run("try smaller".into(), |e| events.push(e))).expect("second run");
        assert!(matches!(
            events.last(),
            Some(AgentEvent::SessionDone { .. })
        ));
    }

    #[test]
    fn torn_tool_input_labelled_tool_calls_is_still_treated_as_a_cut() {
        // The prod failure: the output cap landed mid-input-JSON but the
        // server reported `tool_calls` (not `length`), so `stop_reason`
        // alone says "run this call". The torn JSON is the real evidence.
        // Before the fix this reached the tool layer and came back as
        // `invalid type: string ... expected struct IterateInput`.
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: "{\"note\": \"hearts\", \"source\": \"vec2 pm = p - 0".into(),
            },
            turn_done(StopReason::ToolUse, 10, 8192),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("multiple hearts".into(), |e| events.push(e))).expect("run");

        // Reclassified as a cut: reported as truncation, not executed.
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::Truncated {
                stop_reason: StopReason::MaxTokens,
                dropped_tool_call: true,
            }
        )));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::ToolExecuted { .. })),
            "a torn call must never execute"
        );
        // The dangling tool_use stays out of the transcript so the next
        // turn replays a protocol-valid message. Here the torn call was the
        // turn's only block, so no assistant message is recorded at all.
        assert!(
            !session
                .transcript()
                .messages
                .iter()
                .flat_map(|m| &m.content)
                .any(|b| matches!(b, ContentBlock::ToolUse { .. })),
            "no tool_use may survive without a matching tool_result"
        );
    }

    #[test]
    fn well_formed_tool_input_is_unaffected_by_the_cut_check() {
        // Guard the negative: a complete call on a `tool_calls` turn must
        // still execute normally.
        let input = format!("{{\"source\": {}, \"note\": \"go green\"}}", json!(GREEN));
        let provider = FakeProvider::new(vec![
            vec![
                TurnEvent::ToolUseStart {
                    id: "tu_1".into(),
                    name: "iterate".into(),
                },
                TurnEvent::ToolInputDelta {
                    id: "tu_1".into(),
                    json_fragment: input,
                },
                turn_done(StopReason::ToolUse, 1, 1),
            ],
            vec![turn_done(StopReason::EndTurn, 1, 1)],
        ]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("go green".into(), |e| events.push(e))).expect("run");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ToolExecuted { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::Truncated { .. }))
        );
    }

    #[test]
    fn other_stop_reason_reports_truncation_without_a_tool_drop() {
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::TextDelta("Half a thought".into()),
            turn_done(StopReason::Other("pause_turn".into()), 1, 1),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let mut events = Vec::new();
        block_on(session.run("go".into(), |e| events.push(e))).expect("run");
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::Truncated {
                stop_reason: StopReason::Other(reason),
                dropped_tool_call: false,
            } if reason == "pause_turn"
        )));
    }

    #[test]
    fn abort_mid_tool_loop_synthesizes_cancelled_results() {
        // One turn, two tool calls; the Stop flag flips while the first
        // executes, so the second must never run — but its recorded
        // tool_use still needs an answer for the transcript to replay.
        let input = format!("{{\"source\": {}, \"note\": \"go green\"}}", json!(GREEN));
        let provider = FakeProvider::new(vec![vec![
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: input.clone(),
            },
            TurnEvent::ToolUseStart {
                id: "tu_2".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_2".into(),
                json_fragment: input,
            },
            turn_done(StopReason::ToolUse, 1, 1),
        ]]);
        let mut session = AgentSession::new(&provider, FakeHost::new(RED));
        let abort = session.abort_handle();
        let mut events = Vec::new();
        block_on(session.run("go".into(), |e| {
            if matches!(&e, AgentEvent::ToolExecuted { id, .. } if id == "tu_1") {
                abort.store(true, Ordering::Relaxed);
            }
            events.push(e);
        }))
        .expect("run");

        assert!(events.iter().any(|e| matches!(e, AgentEvent::Aborted)));
        // The results message answers BOTH calls: the executed first one
        // and a cancelled second one.
        let results = &session.transcript().messages[2];
        assert_eq!(results.content.len(), 2, "{:?}", results.content);
        assert!(matches!(
            &results.content[0],
            ContentBlock::ToolResult { tool_use_id, is_error: None, .. } if tool_use_id == "tu_1"
        ));
        match &results.content[1] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tu_2");
                assert!(content.contains("cancelled"), "{content}");
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

        fn stage_source<'a>(
            &'a mut self,
            source: &'a str,
        ) -> HostFuture<'a, Result<(), HostError>> {
            Box::pin(async move {
                self.staged.borrow_mut().push(source.to_string());
                *self.source.borrow_mut() = source.to_string();
                Ok(())
            })
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
