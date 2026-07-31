//! [`OpenAiCompatProvider`]: streaming `/chat/completions` implementation
//! of [`ModelProvider`] over the shared [`HttpSseTransport`] seam.
//!
//! Speaks the OpenAI Chat Completions dialect, which local/OSS servers
//! (Ollama, LM Studio, llama.cpp, vLLM) also implement — `base_url` plus an
//! optional API key covers all of them. Mirrors the Anthropic provider's
//! shape: a poll-driven [`TurnDriver`] state machine in a runtime-neutral
//! `unfold` stream, with the same single-retry policy (429/5xx/network
//! before any event was emitted).

use std::collections::{BTreeMap, VecDeque};

use futures_util::StreamExt;

use crate::provider::http_transport::{ByteStream, HttpRequest, HttpSseTransport, TransportError};
use crate::provider::model_provider::{
    BoxStream, ModelProvider, StopReason, TokenUsage, TurnEvent, TurnRequest,
};
use crate::provider::openai_compat::openai_compat_wire::{
    ChatCompletionsRequest, ErrorBody, StreamChunk, StreamOptions, WireTool, map_finish_reason,
    wire_messages,
};
use crate::provider::sse_parser::{SseEvent, SseParser};

/// Default API origin (OpenAI itself; local servers override this).
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// The stream's terminator payload.
const DONE_PAYLOAD: &str = "[DONE]";
/// Backoff before the single retry.
const RETRY_BACKOFF_MS: u32 = 500;
/// Cap on how much of a non-2xx response body is read for the error message.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// OpenAI-compatible connection settings. No default model exists: model
/// ids are provider-specific and never guessed here — Studio requires the
/// field before the agent becomes available.
#[derive(Clone, Debug)]
pub struct OpenAiCompatConfig {
    /// API origin including any path prefix (e.g. `https://api.openai.com/v1`,
    /// `http://localhost:11434/v1` for Ollama).
    pub base_url: String,
    /// Bearer token. `None` omits the `Authorization` header entirely
    /// (local servers).
    pub api_key: Option<String>,
    /// The model id, as named by the serving provider.
    pub model: String,
    /// Additional request headers, e.g. OpenRouter's `HTTP-Referer`/`X-Title`
    /// attribution pair. Names must not be browser-forbidden header names.
    pub extra_headers: Vec<(String, String)>,
}

/// The OpenAI-compatible streaming provider.
pub struct OpenAiCompatProvider<T: HttpSseTransport> {
    config: OpenAiCompatConfig,
    transport: T,
}

impl<T: HttpSseTransport> OpenAiCompatProvider<T> {
    pub fn new(config: OpenAiCompatConfig, transport: T) -> Self {
        Self { config, transport }
    }

    fn build_request(&self, req: &TurnRequest) -> HttpRequest {
        let body = serde_json::to_string(&ChatCompletionsRequest {
            model: &self.config.model,
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
            // No per-turn output ceiling is sent. This dialect serves
            // arbitrary servers and local models, so any fixed number is a
            // guess in both directions: too low silently truncates long
            // edits mid-tool-call (a full shader payload does not fit in
            // the 8k this used to send), too high is rejected or clamped by
            // servers whose model max sits below it. Omitting the field
            // lets each server apply its own model max, which is the answer
            // we were trying to guess. Consequence: a runaway turn is
            // bounded by the run's turn limit rather than a token ceiling.
            max_completion_tokens: None,
            messages: wire_messages(&req.system, &req.messages),
            tools: req.tools.iter().map(WireTool::from_def).collect(),
        })
        .expect("request serialization is infallible");
        let mut headers = vec![("content-type".into(), "application/json".into())];
        if let Some(key) = &self.config.api_key {
            headers.push(("authorization".into(), format!("Bearer {key}")));
        }
        headers.extend(self.config.extra_headers.iter().cloned());
        HttpRequest {
            url: format!(
                "{}/chat/completions",
                self.config.base_url.trim_end_matches('/')
            ),
            headers,
            body,
        }
    }
}

impl<T: HttpSseTransport> ModelProvider for OpenAiCompatProvider<T> {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        let driver = TurnDriver::new(&self.transport, self.build_request(&req));
        Box::pin(futures_util::stream::unfold(driver, |mut d| async move {
            d.next_event().await.map(|ev| (ev, d))
        }))
    }
}

/// Turn phases.
enum Phase {
    /// Not yet connected (or about to retry).
    Start,
    /// Receiving the SSE body.
    Streaming { body: ByteStream },
    /// Terminal; the stream ends once `pending` drains.
    Finished,
}

/// Poll-driven turn state machine: each `next_event` call makes progress
/// until one [`TurnEvent`] is available.
struct TurnDriver<'a, T: HttpSseTransport> {
    transport: &'a T,
    request: HttpRequest,
    phase: Phase,
    parser: SseParser,
    pending: VecDeque<TurnEvent>,
    /// Tool-call index → tool_use id, for routing argument fragments.
    tool_ids: BTreeMap<usize, String>,
    finish_reason: Option<StopReason>,
    usage: TokenUsage,
    /// Whether any event has been handed to the consumer (disables retry).
    emitted_any: bool,
    /// Whether the single retry has been spent.
    retried: bool,
}

impl<'a, T: HttpSseTransport> TurnDriver<'a, T> {
    fn new(transport: &'a T, request: HttpRequest) -> Self {
        Self {
            transport,
            request,
            phase: Phase::Start,
            parser: SseParser::new(),
            pending: VecDeque::new(),
            tool_ids: BTreeMap::new(),
            finish_reason: None,
            usage: TokenUsage::default(),
            emitted_any: false,
            retried: false,
        }
    }

    async fn next_event(&mut self) -> Option<TurnEvent> {
        loop {
            if let Some(ev) = self.pending.pop_front() {
                self.emitted_any = true;
                return Some(ev);
            }
            match &mut self.phase {
                Phase::Finished => return None,
                Phase::Start => self.connect().await,
                Phase::Streaming { body } => {
                    let chunk = body.next().await;
                    self.consume_chunk(chunk);
                }
            }
        }
    }

    /// POST the request; on success move to `Streaming`, otherwise apply
    /// the retry policy.
    async fn connect(&mut self) {
        match self.transport.post(self.request.clone()).await {
            Ok(resp) if resp.status == 200 => {
                self.phase = Phase::Streaming { body: resp.body };
            }
            Ok(resp) => {
                let status = resp.status;
                let text = read_body_capped(resp.body).await;
                let message = match serde_json::from_str::<ErrorBody>(&text) {
                    Ok(e) => format!("API error {status} ({}): {}", e.error.kind, e.error.message),
                    Err(_) => format!("API error {status}: {}", truncate(&text, 300)),
                };
                let retryable = status == 429 || status >= 500;
                self.fail_or_retry(message, retryable).await;
            }
            Err(TransportError { message }) => {
                self.fail_or_retry(format!("network error: {message}"), true)
                    .await;
            }
        }
    }

    /// One retry with backoff if the failure is retryable and nothing has
    /// been emitted yet; otherwise surface the error and finish.
    async fn fail_or_retry(&mut self, message: String, retryable: bool) {
        if retryable && !self.retried && !self.emitted_any {
            self.retried = true;
            self.transport.sleep_ms(RETRY_BACKOFF_MS).await;
            self.phase = Phase::Start;
            return;
        }
        self.pending
            .push_back(TurnEvent::ProviderError { message, retryable });
        self.phase = Phase::Finished;
    }

    /// Handle one body-stream step (chunk, error, or EOF).
    fn consume_chunk(&mut self, chunk: Option<Result<Vec<u8>, TransportError>>) {
        match chunk {
            Some(Ok(bytes)) => {
                for ev in self.parser.feed(&bytes) {
                    self.on_sse_event(ev);
                }
            }
            Some(Err(TransportError { message })) => {
                self.pending.push_back(TurnEvent::ProviderError {
                    message: format!("stream error: {message}"),
                    retryable: true,
                });
                self.phase = Phase::Finished;
            }
            None => {
                // EOF without `[DONE]`. A finish_reason means the turn
                // completed and only the terminator was dropped (some local
                // servers are sloppy here) — accept it. Otherwise the turn
                // never finished.
                if self.finish_reason.is_some() {
                    self.finish_turn();
                } else {
                    self.pending.push_back(TurnEvent::ProviderError {
                        message: "stream ended before completion".into(),
                        retryable: true,
                    });
                    self.phase = Phase::Finished;
                }
            }
        }
    }

    /// Map one SSE `data:` payload onto zero or more [`TurnEvent`]s.
    fn on_sse_event(&mut self, ev: SseEvent) {
        if ev.data.is_empty() {
            return;
        }
        if ev.data == DONE_PAYLOAD {
            self.finish_turn();
            return;
        }
        let chunk: StreamChunk = match serde_json::from_str(&ev.data) {
            Ok(c) => c,
            Err(_) => {
                // Some servers surface errors as a JSON error object inside
                // the stream; anything else is malformed.
                let (message, retryable) = match serde_json::from_str::<ErrorBody>(&ev.data) {
                    Ok(e) => (
                        format!("API stream error ({}): {}", e.error.kind, e.error.message),
                        false,
                    ),
                    Err(e) => (format!("malformed stream payload: {e}"), false),
                };
                self.pending
                    .push_back(TurnEvent::ProviderError { message, retryable });
                self.phase = Phase::Finished;
                return;
            }
        };
        if let Some(usage) = chunk.usage {
            // This dialect's `prompt_tokens` INCLUDES cached tokens;
            // subtract them so the neutral buckets stay disjoint (input =
            // uncached remainder, matching Anthropic semantics). Caching is
            // automatic server-side here, so there is never a write bucket.
            let cached = usage.cached_tokens();
            self.usage = TokenUsage {
                input_tokens: usage.prompt_tokens.saturating_sub(cached),
                output_tokens: usage.completion_tokens,
                cache_write_tokens: 0,
                cache_read_tokens: cached,
            };
        }
        let Some(choice) = chunk.choices.into_iter().next() else {
            return; // the trailing usage-only chunk
        };
        // Reasoning rides ahead of the visible answer on servers that emit
        // it (DeepSeek/vLLM/Ollama `reasoning_content`, OpenRouter
        // `reasoning`); nothing special is sent on the request — some
        // servers reject unknown fields, so enabling is server-side.
        if let Some(fragment) = choice.delta.reasoning_fragment() {
            self.pending
                .push_back(TurnEvent::ThinkingDelta(fragment.to_string()));
        }
        if let Some(text) = choice.delta.content
            && !text.is_empty()
        {
            self.pending.push_back(TurnEvent::TextDelta(text));
        }
        for call in choice.delta.tool_calls {
            let id = match self.tool_ids.get(&call.index) {
                Some(id) => id.clone(),
                None => {
                    // First fragment for this index: id and name arrive
                    // here (a missing id gets a synthesized one so the
                    // session can still correlate fragments).
                    let id = call
                        .id
                        .unwrap_or_else(|| format!("tool_call_{}", call.index));
                    self.tool_ids.insert(call.index, id.clone());
                    self.pending.push_back(TurnEvent::ToolUseStart {
                        id: id.clone(),
                        name: call.function.name.unwrap_or_default(),
                    });
                    id
                }
            };
            if let Some(fragment) = call.function.arguments
                && !fragment.is_empty()
            {
                self.pending.push_back(TurnEvent::ToolInputDelta {
                    id,
                    json_fragment: fragment,
                });
            }
        }
        if let Some(reason) = choice.finish_reason {
            self.finish_reason = Some(map_finish_reason(&reason));
        }
    }

    /// Emit `TurnDone` from the recorded finish reason and usage.
    fn finish_turn(&mut self) {
        self.pending.push_back(TurnEvent::TurnDone {
            stop_reason: self
                .finish_reason
                .take()
                .unwrap_or(StopReason::Other("missing finish_reason".into())),
            usage: self.usage,
        });
        self.phase = Phase::Finished;
    }
}

/// Read a body stream to completion, capped at [`MAX_ERROR_BODY_BYTES`].
async fn read_body_capped(mut body: ByteStream) -> String {
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = body.next().await {
        bytes.extend_from_slice(&chunk);
        if bytes.len() >= MAX_ERROR_BODY_BYTES {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use futures_util::stream;

    use super::*;
    use crate::provider::http_transport::{HttpResponse, LocalBoxFuture};
    use crate::provider::model_provider::ChatMessage;

    #[test]
    fn text_turn_streams_deltas_and_done() {
        let transport = FakeTransport::respond_once(200, vec![TEXT_TURN.as_bytes().to_vec()]);
        let events = run_with_key(&transport, Some("sk-test"));
        assert_eq!(
            events,
            vec![
                TurnEvent::TextDelta("Hel".into()),
                TurnEvent::TextDelta("lo".into()),
                TurnEvent::TurnDone {
                    stop_reason: StopReason::EndTurn,
                    usage: TokenUsage {
                        input_tokens: 25,
                        output_tokens: 12,
                        ..TokenUsage::default()
                    }
                }
            ]
        );
        let reqs = transport.requests.borrow();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].url.ends_with("/chat/completions"));
        assert!(
            reqs[0]
                .headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer sk-test")
        );
        assert!(reqs[0].body.contains("\"stream\":true"));
        assert!(reqs[0].body.contains("\"include_usage\":true"));
        // No output ceiling is sent: each server applies its own model max.
        assert!(!reqs[0].body.contains("max_completion_tokens"));
    }

    #[test]
    fn extra_headers_ride_along() {
        let transport = FakeTransport::respond_once(200, vec![TEXT_TURN.as_bytes().to_vec()]);
        let provider = OpenAiCompatProvider::new(
            OpenAiCompatConfig {
                base_url: DEFAULT_BASE_URL.to_string(),
                api_key: None,
                model: "gpt-test".to_string(),
                extra_headers: vec![("X-Title".to_string(), "LightPlayer Studio".to_string())],
            },
            &transport,
        );
        let req = TurnRequest {
            system: "sys".into(),
            messages: vec![ChatMessage::user_text("hi")],
            tools: vec![],
        };
        futures_executor::block_on(StreamExt::collect::<Vec<_>>(provider.run_turn(req)));
        let reqs = transport.requests.borrow();
        assert!(
            reqs[0]
                .headers
                .iter()
                .any(|(k, v)| k == "X-Title" && v == "LightPlayer Studio"),
            "{:?}",
            reqs[0].headers
        );
    }

    #[test]
    fn no_key_omits_the_authorization_header() {
        let transport = FakeTransport::respond_once(200, vec![TEXT_TURN.as_bytes().to_vec()]);
        let events = run_with_key(&transport, None);
        assert!(matches!(events.last(), Some(TurnEvent::TurnDone { .. })));
        let reqs = transport.requests.borrow();
        assert!(
            !reqs[0]
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization")),
            "{:?}",
            reqs[0].headers
        );
    }

    #[test]
    fn tool_turn_accumulates_split_argument_fragments_by_index() {
        // Chunks split mid-event to exercise incremental parsing end-to-end.
        let bytes = TOOL_TURN.as_bytes();
        let mid = bytes.len() / 2;
        let transport =
            FakeTransport::respond_once(200, vec![bytes[..mid].to_vec(), bytes[mid..].to_vec()]);
        let events = run_with_key(&transport, Some("sk-test"));
        assert_eq!(
            events,
            vec![
                TurnEvent::TextDelta("Trying.".into()),
                TurnEvent::ToolUseStart {
                    id: "call_1".into(),
                    name: "iterate".into()
                },
                TurnEvent::ToolInputDelta {
                    id: "call_1".into(),
                    json_fragment: "{\"note\":".into()
                },
                TurnEvent::ToolInputDelta {
                    id: "call_1".into(),
                    json_fragment: "\"blue\"}".into()
                },
                TurnEvent::TurnDone {
                    stop_reason: StopReason::ToolUse,
                    usage: TokenUsage {
                        input_tokens: 40,
                        output_tokens: 30,
                        ..TokenUsage::default()
                    }
                }
            ]
        );
    }

    #[test]
    fn reasoning_deltas_stream_as_thinking_before_the_answer() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Weighing \"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning\":\"palettes.\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Warmer.\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run_with_key(&transport, None);
        assert_eq!(
            events,
            vec![
                TurnEvent::ThinkingDelta("Weighing ".into()),
                TurnEvent::ThinkingDelta("palettes.".into()),
                TurnEvent::TextDelta("Warmer.".into()),
                TurnEvent::TurnDone {
                    stop_reason: StopReason::EndTurn,
                    usage: TokenUsage::default()
                }
            ]
        );
        // No reasoning-enablement field rides the request (unsafe on some
        // compat servers; OpenRouter opt-in is a follow-up).
        let reqs = transport.requests.borrow();
        assert!(!reqs[0].body.contains("\"reasoning\""), "{}", reqs[0].body);
    }

    #[test]
    fn cached_prompt_tokens_map_to_cache_read_and_are_subtracted_from_input() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":12,",
            "\"prompt_tokens_details\":{\"cached_tokens\":768}}}\n\n",
            "data: [DONE]\n\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run_with_key(&transport, Some("sk-test"));
        assert_eq!(
            events.last(),
            Some(&TurnEvent::TurnDone {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    // 1000 reported prompt tokens minus the 768 cached ones.
                    input_tokens: 232,
                    output_tokens: 12,
                    cache_write_tokens: 0,
                    cache_read_tokens: 768,
                }
            })
        );
    }

    #[test]
    fn missing_usage_reports_zeros() {
        // No usage chunk before [DONE] (local servers that ignore
        // stream_options.include_usage).
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run_with_key(&transport, None);
        assert_eq!(
            events.last(),
            Some(&TurnEvent::TurnDone {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage::default()
            })
        );
    }

    #[test]
    fn eof_after_finish_reason_completes_the_turn() {
        // Sloppy server: finish_reason arrives but [DONE] never does.
        let sse = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run_with_key(&transport, None);
        assert!(
            matches!(events.last(), Some(TurnEvent::TurnDone { stop_reason, .. })
                if *stop_reason == StopReason::EndTurn),
            "{events:?}"
        );
    }

    #[test]
    fn eof_without_finish_reason_is_a_retryable_error() {
        let sse =
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n";
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run_with_key(&transport, None);
        assert!(
            matches!(events.last(), Some(TurnEvent::ProviderError { message, retryable })
                if message.contains("before completion") && *retryable),
            "{events:?}"
        );
    }

    #[test]
    fn retries_once_on_429_then_succeeds() {
        let transport = FakeTransport::new(vec![
            Scripted::Respond {
                status: 429,
                chunks: vec![
                    br#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#.to_vec(),
                ],
            },
            Scripted::Respond {
                status: 200,
                chunks: vec![TEXT_TURN.as_bytes().to_vec()],
            },
        ]);
        let events = run_with_key(&transport, Some("sk-test"));
        assert!(
            matches!(events.last(), Some(TurnEvent::TurnDone { .. })),
            "{events:?}"
        );
        assert_eq!(transport.requests.borrow().len(), 2);
        assert_eq!(*transport.sleeps.borrow(), vec![RETRY_BACKOFF_MS]);
    }

    #[test]
    fn double_network_failure_surfaces_retryable_error() {
        let transport = FakeTransport::new(vec![
            Scripted::Fail("connection refused".into()),
            Scripted::Fail("connection refused".into()),
        ]);
        let events = run_with_key(&transport, None);
        assert_eq!(events.len(), 1);
        match &events[0] {
            TurnEvent::ProviderError { message, retryable } => {
                assert!(message.contains("connection refused"), "{message}");
                assert!(retryable);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(transport.requests.borrow().len(), 2);
    }

    #[test]
    fn non_retryable_status_fails_without_retry() {
        let transport = FakeTransport::respond_once(
            401,
            vec![
                br#"{"error":{"type":"invalid_request_error","message":"Incorrect API key"}}"#
                    .to_vec(),
            ],
        );
        let events = run_with_key(&transport, Some("bad"));
        match &events[0] {
            TurnEvent::ProviderError { message, retryable } => {
                assert!(message.contains("invalid_request_error"), "{message}");
                assert!(message.contains("Incorrect API key"), "{message}");
                assert!(!retryable);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(transport.requests.borrow().len(), 1);
        assert!(transport.sleeps.borrow().is_empty());
    }

    // -- helpers ----------------------------------------------------------

    const TEXT_TURN: &str = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":25,\"completion_tokens\":12}}\n\n",
        "data: [DONE]\n\n",
    );

    const TOOL_TURN: &str = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Trying.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"iterate\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"note\\\":\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"blue\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":30}}\n\n",
        "data: [DONE]\n\n",
    );

    fn run_with_key(transport: &FakeTransport, api_key: Option<&str>) -> Vec<TurnEvent> {
        let provider = OpenAiCompatProvider::new(
            OpenAiCompatConfig {
                base_url: DEFAULT_BASE_URL.to_string(),
                api_key: api_key.map(str::to_string),
                model: "gpt-test".to_string(),
                extra_headers: Vec::new(),
            },
            transport,
        );
        let req = TurnRequest {
            system: "sys".into(),
            messages: vec![ChatMessage::user_text("hi")],
            tools: vec![],
        };
        futures_executor::block_on(StreamExt::collect::<Vec<_>>(provider.run_turn(req)))
    }

    enum Scripted {
        Fail(String),
        Respond { status: u16, chunks: Vec<Vec<u8>> },
    }

    struct FakeTransport {
        responses: RefCell<VecDeque<Scripted>>,
        requests: RefCell<Vec<HttpRequest>>,
        sleeps: RefCell<Vec<u32>>,
    }

    impl FakeTransport {
        fn new(script: Vec<Scripted>) -> Self {
            Self {
                responses: RefCell::new(script.into()),
                requests: RefCell::new(Vec::new()),
                sleeps: RefCell::new(Vec::new()),
            }
        }

        fn respond_once(status: u16, chunks: Vec<Vec<u8>>) -> Self {
            Self::new(vec![Scripted::Respond { status, chunks }])
        }
    }

    impl HttpSseTransport for &FakeTransport {
        fn post(
            &self,
            req: HttpRequest,
        ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
            self.requests.borrow_mut().push(req);
            let next = self
                .responses
                .borrow_mut()
                .pop_front()
                .expect("unscripted request");
            Box::pin(async move {
                match next {
                    Scripted::Fail(message) => Err(TransportError { message }),
                    Scripted::Respond { status, chunks } => Ok(HttpResponse {
                        status,
                        body: Box::pin(stream::iter(chunks.into_iter().map(Ok))),
                    }),
                }
            })
        }

        fn sleep_ms(&self, ms: u32) -> LocalBoxFuture<'static, ()> {
            self.sleeps.borrow_mut().push(ms);
            Box::pin(async {})
        }
    }
}
