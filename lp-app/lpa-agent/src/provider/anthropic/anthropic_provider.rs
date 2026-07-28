//! [`AnthropicProvider`]: streaming `/v1/messages` implementation of
//! [`ModelProvider`] over an injected [`HttpSseTransport`].
//!
//! The turn is driven by a small state machine ([`TurnDriver`]) wrapped in a
//! runtime-neutral `unfold` stream: no spawning, no executor assumptions —
//! whatever polls the stream drives the transport. Retry policy: one retry
//! with a short backoff on retryable failures (429/5xx/network) that occur
//! before any event was emitted; everything else surfaces as
//! [`TurnEvent::ProviderError`].

use std::collections::{BTreeMap, VecDeque};

use futures_util::StreamExt;

use crate::provider::anthropic::anthropic_wire::{
    ErrorBody, MessagesRequest, SsePayload, WireBlockDelta, WireContentBlockStart, map_stop_reason,
};
use crate::provider::http_transport::{ByteStream, HttpRequest, HttpSseTransport, TransportError};
use crate::provider::model_provider::{
    BoxStream, ModelProvider, StopReason, TokenUsage, TurnEvent, TurnRequest,
};
use crate::provider::sse_parser::{SseEvent, SseParser};

/// Default model id.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";
/// Default API origin (overridable for tests).
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// The `anthropic-version` header value (shared with model discovery).
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Per-turn output-token ceiling (`max_tokens`). Current Claude models
/// support ≥64k output tokens; 32k gives a long shader landing inside an
/// `iterate` input JSON — plus adaptive thinking, which spends from the
/// same output budget — generous headroom while still bounding a runaway
/// turn. The old 8k budget truncated tool calls mid-JSON on big shaders,
/// ending runs silently with `stop_reason: max_tokens`.
pub const ANTHROPIC_MAX_OUTPUT_TOKENS: u32 = 32_000;
/// Backoff before the single retry.
const RETRY_BACKOFF_MS: u32 = 500;
/// Cap on how much of a non-2xx response body is read for the error message.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// Anthropic connection settings. Studio maps its persisted settings into
/// this struct (this crate has no settings/UI code).
#[derive(Clone, Debug)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl AnthropicConfig {
    /// Config with the default model and base URL.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }
}

/// The Anthropic streaming provider.
pub struct AnthropicProvider<T: HttpSseTransport> {
    config: AnthropicConfig,
    transport: T,
}

impl<T: HttpSseTransport> AnthropicProvider<T> {
    pub fn new(config: AnthropicConfig, transport: T) -> Self {
        Self { config, transport }
    }

    fn build_request(&self, req: &TurnRequest) -> HttpRequest {
        let body = MessagesRequest {
            model: &self.config.model,
            max_tokens: ANTHROPIC_MAX_OUTPUT_TOKENS,
            stream: true,
            system: &req.system,
            messages: &req.messages,
            tools: &req.tools,
        }
        .body()
        .to_string();
        HttpRequest {
            url: format!("{}/v1/messages", self.config.base_url.trim_end_matches('/')),
            headers: vec![
                ("x-api-key".into(), self.config.api_key.clone()),
                ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
                ("content-type".into(), "application/json".into()),
                // Required for CORS from the browser; harmless on host.
                (
                    "anthropic-dangerous-direct-browser-access".into(),
                    "true".into(),
                ),
            ],
            body,
        }
    }
}

impl<T: HttpSseTransport> ModelProvider for AnthropicProvider<T> {
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
    /// Content-block index → tool_use id, for routing `input_json_delta`.
    tool_ids: BTreeMap<usize, String>,
    stop_reason: Option<StopReason>,
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
            stop_reason: None,
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

    /// POST the request; on success move to `Streaming`, otherwise apply the
    /// retry policy.
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
                // EOF without message_stop: the turn never completed.
                self.pending.push_back(TurnEvent::ProviderError {
                    message: "stream ended before message_stop".into(),
                    retryable: true,
                });
                self.phase = Phase::Finished;
            }
        }
    }

    /// Map one SSE event onto zero or more [`TurnEvent`]s.
    fn on_sse_event(&mut self, ev: SseEvent) {
        if ev.data.is_empty() {
            return;
        }
        let payload: SsePayload = match serde_json::from_str(&ev.data) {
            Ok(p) => p,
            Err(e) => {
                self.pending.push_back(TurnEvent::ProviderError {
                    message: format!("malformed stream payload: {e}"),
                    retryable: false,
                });
                self.phase = Phase::Finished;
                return;
            }
        };
        match payload {
            SsePayload::MessageStart { message } => {
                if let Some(u) = message.usage {
                    self.usage.input_tokens = u.input_tokens;
                    self.usage.cache_write_tokens = u.cache_creation_input_tokens;
                    self.usage.cache_read_tokens = u.cache_read_input_tokens;
                }
            }
            SsePayload::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                WireContentBlockStart::Text { text } => {
                    if !text.is_empty() {
                        self.pending.push_back(TurnEvent::TextDelta(text));
                    }
                }
                WireContentBlockStart::ToolUse { id, name } => {
                    self.tool_ids.insert(index, id.clone());
                    self.pending.push_back(TurnEvent::ToolUseStart { id, name });
                }
                WireContentBlockStart::Thinking { thinking } => {
                    if !thinking.is_empty() {
                        self.pending.push_back(TurnEvent::ThinkingDelta(thinking));
                    }
                }
                WireContentBlockStart::RedactedThinking { data } => {
                    self.pending.push_back(TurnEvent::RedactedThinking(data));
                }
                WireContentBlockStart::Unknown => {}
            },
            SsePayload::ContentBlockDelta { index, delta } => match delta {
                WireBlockDelta::TextDelta { text } => {
                    self.pending.push_back(TurnEvent::TextDelta(text));
                }
                WireBlockDelta::InputJsonDelta { partial_json } => {
                    if let Some(id) = self.tool_ids.get(&index) {
                        self.pending.push_back(TurnEvent::ToolInputDelta {
                            id: id.clone(),
                            json_fragment: partial_json,
                        });
                    }
                }
                WireBlockDelta::ThinkingDelta { thinking } => {
                    self.pending.push_back(TurnEvent::ThinkingDelta(thinking));
                }
                WireBlockDelta::SignatureDelta { signature } => {
                    self.pending
                        .push_back(TurnEvent::ThinkingSignature(signature));
                }
                WireBlockDelta::Unknown => {}
            },
            SsePayload::ContentBlockStop { .. } | SsePayload::Ping | SsePayload::Unknown => {}
            SsePayload::MessageDelta { delta, usage } => {
                if let Some(s) = delta.stop_reason {
                    self.stop_reason = Some(map_stop_reason(&s));
                }
                if let Some(u) = usage {
                    // Cumulative output tokens for the turn; input-side
                    // fields override `message_start` values when the
                    // final delta repeats them.
                    self.usage.output_tokens = u.output_tokens;
                    if let Some(input) = u.input_tokens {
                        self.usage.input_tokens = input;
                    }
                    if let Some(write) = u.cache_creation_input_tokens {
                        self.usage.cache_write_tokens = write;
                    }
                    if let Some(read) = u.cache_read_input_tokens {
                        self.usage.cache_read_tokens = read;
                    }
                }
            }
            SsePayload::MessageStop => {
                self.pending.push_back(TurnEvent::TurnDone {
                    stop_reason: self
                        .stop_reason
                        .take()
                        .unwrap_or(StopReason::Other("missing stop_reason".into())),
                    usage: self.usage,
                });
                self.phase = Phase::Finished;
            }
            SsePayload::Error { error } => {
                self.pending.push_back(TurnEvent::ProviderError {
                    message: format!("API stream error ({}): {}", error.kind, error.message),
                    retryable: error.kind == "overloaded_error",
                });
                self.phase = Phase::Finished;
            }
        }
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
    use crate::provider::http_transport::HttpResponse;
    use crate::provider::model_provider::ChatMessage;

    #[test]
    fn text_turn_streams_deltas_and_done() {
        let transport = FakeTransport::respond_once(200, vec![TEXT_TURN.as_bytes().to_vec()]);
        let events = run(&transport);
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
        assert!(reqs[0].url.ends_with("/v1/messages"));
        assert!(
            reqs[0]
                .headers
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "test-key")
        );
        assert!(reqs[0].body.contains("\"stream\":true"));
    }

    #[test]
    fn tool_turn_routes_input_deltas_by_block_index() {
        // Chunks split mid-event to exercise incremental parsing end-to-end.
        let bytes = TOOL_TURN.as_bytes();
        let mid = bytes.len() / 2;
        let transport =
            FakeTransport::respond_once(200, vec![bytes[..mid].to_vec(), bytes[mid..].to_vec()]);
        let events = run(&transport);
        assert_eq!(
            events,
            vec![
                TurnEvent::TextDelta("Trying.".into()),
                TurnEvent::ToolUseStart {
                    id: "tu_1".into(),
                    name: "iterate".into()
                },
                TurnEvent::ToolInputDelta {
                    id: "tu_1".into(),
                    json_fragment: "{\"note\":".into()
                },
                TurnEvent::ToolInputDelta {
                    id: "tu_1".into(),
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
    fn thinking_turn_streams_thinking_then_signature_then_text() {
        // Chunks split mid-event to exercise incremental parsing end-to-end.
        let bytes = THINKING_TURN.as_bytes();
        let mid = bytes.len() / 2;
        let transport =
            FakeTransport::respond_once(200, vec![bytes[..mid].to_vec(), bytes[mid..].to_vec()]);
        let events = run(&transport);
        assert_eq!(
            events,
            vec![
                TurnEvent::ThinkingDelta("Let me consider ".into()),
                TurnEvent::ThinkingDelta("the palette.".into()),
                TurnEvent::ThinkingSignature("EqQBsig".into()),
                TurnEvent::RedactedThinking("EmwKopaque".into()),
                TurnEvent::TextDelta("Warmer it is.".into()),
                TurnEvent::TurnDone {
                    stop_reason: StopReason::EndTurn,
                    usage: TokenUsage {
                        input_tokens: 30,
                        output_tokens: 90,
                        ..TokenUsage::default()
                    }
                }
            ]
        );
        // The request opted into adaptive thinking with summarized display.
        let reqs = transport.requests.borrow();
        let body: serde_json::Value = serde_json::from_str(&reqs[0].body).expect("json body");
        assert_eq!(
            body["thinking"],
            serde_json::json!({"type": "adaptive", "display": "summarized"})
        );
    }

    #[test]
    fn cache_usage_from_message_start_lands_in_turn_usage() {
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{",
            "\"input_tokens\":21,\"cache_creation_input_tokens\":2100,",
            "\"cache_read_input_tokens\":18000,\"output_tokens\":1}}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":9}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run(&transport);
        assert_eq!(
            events,
            vec![TurnEvent::TurnDone {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 21,
                    output_tokens: 9,
                    cache_write_tokens: 2100,
                    cache_read_tokens: 18000,
                }
            }]
        );
    }

    #[test]
    fn message_delta_usage_overrides_input_side_fields_when_present() {
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},",
            "\"usage\":{\"output_tokens\":9,\"input_tokens\":21,",
            "\"cache_creation_input_tokens\":2100,\"cache_read_input_tokens\":18000}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run(&transport);
        assert_eq!(
            events,
            vec![TurnEvent::TurnDone {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 21,
                    output_tokens: 9,
                    cache_write_tokens: 2100,
                    cache_read_tokens: 18000,
                }
            }]
        );
    }

    #[test]
    fn request_body_carries_cache_breakpoints() {
        let transport = FakeTransport::respond_once(200, vec![TEXT_TURN.as_bytes().to_vec()]);
        let _ = run(&transport);
        let reqs = transport.requests.borrow();
        let body: serde_json::Value = serde_json::from_str(&reqs[0].body).expect("json body");
        // System prompt is the block form with a cache marker...
        assert_eq!(
            body["system"],
            serde_json::json!([{
                "type": "text", "text": "sys",
                "cache_control": {"type": "ephemeral"},
            }])
        );
        // ...and the last content block of the last message carries the
        // rolling marker.
        let last_block = body["messages"]
            .as_array()
            .and_then(|m| m.last())
            .and_then(|m| m["content"].as_array())
            .and_then(|c| c.last())
            .expect("last block");
        assert_eq!(
            last_block["cache_control"],
            serde_json::json!({"type": "ephemeral"})
        );
    }

    #[test]
    fn retries_once_on_429_then_succeeds() {
        let transport = FakeTransport::new(vec![
            Scripted::Respond {
                status: 429,
                chunks: vec![
                    br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                        .to_vec(),
                ],
            },
            Scripted::Respond {
                status: 200,
                chunks: vec![TEXT_TURN.as_bytes().to_vec()],
            },
        ]);
        let events = run(&transport);
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
        let events = run(&transport);
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
                br#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#
                    .to_vec(),
            ],
        );
        let events = run(&transport);
        match &events[0] {
            TurnEvent::ProviderError { message, retryable } => {
                assert!(message.contains("authentication_error"), "{message}");
                assert!(message.contains("bad key"), "{message}");
                assert!(!retryable);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(transport.requests.borrow().len(), 1);
        assert!(transport.sleeps.borrow().is_empty());
    }

    #[test]
    fn premature_stream_end_is_an_error() {
        let transport = FakeTransport::respond_once(
            200,
            vec![
                b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n"
                    .to_vec(),
            ],
        );
        let events = run(&transport);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], TurnEvent::ProviderError { message, .. }
                if message.contains("before message_stop")),
            "{events:?}"
        );
    }

    #[test]
    fn api_error_event_ends_the_turn() {
        let sse = concat!(
            "event: error\n",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n",
            "\n",
        );
        let transport = FakeTransport::respond_once(200, vec![sse.as_bytes().to_vec()]);
        let events = run(&transport);
        assert_eq!(
            events,
            vec![TurnEvent::ProviderError {
                message: "API stream error (overloaded_error): busy".into(),
                retryable: true
            }]
        );
    }

    // -- helpers ----------------------------------------------------------

    const TEXT_TURN: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":25,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: ping\ndata: {\"type\": \"ping\"}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    const THINKING_TURN: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":30,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me consider \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"the palette.\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"EqQBsig\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"EmwKopaque\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Warmer it is.\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        // Thinking tokens are OUTPUT tokens: the cumulative figure covers
        // thinking + text together, so the existing bucket already adds up.
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":90}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    const TOOL_TURN: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":40,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Trying.\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"iterate\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"note\\\":\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"blue\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":30}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    fn run(transport: &FakeTransport) -> Vec<TurnEvent> {
        let provider = AnthropicProvider::new(AnthropicConfig::new("test-key"), transport);
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
        ) -> crate::provider::http_transport::LocalBoxFuture<
            'static,
            Result<HttpResponse, TransportError>,
        > {
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

        fn sleep_ms(
            &self,
            ms: u32,
        ) -> crate::provider::http_transport::LocalBoxFuture<'static, ()> {
            self.sleeps.borrow_mut().push(ms);
            Box::pin(async {})
        }
    }
}
