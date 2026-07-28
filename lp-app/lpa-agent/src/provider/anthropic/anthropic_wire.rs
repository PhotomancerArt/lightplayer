//! Anthropic `/v1/messages` wire types: request serialization (with prompt-
//! cache breakpoints) and streamed SSE payload deserialization. Pure serde
//! types plus the body builder, no IO.

use serde::Deserialize;
use serde_json::json;

use crate::provider::model_provider::{ChatMessage, StopReason, ToolDef};

/// The streaming request parameters. Transcript [`ChatMessage`]s and
/// [`ToolDef`]s serialize directly to the wire shapes (`{ role, content:
/// [...] }`, `{ name, description, input_schema }`); [`Self::body`] builds
/// the JSON body with prompt-cache breakpoints injected.
#[derive(Debug)]
pub struct MessagesRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub stream: bool,
    pub system: &'a str,
    pub messages: &'a [ChatMessage],
    pub tools: &'a [ToolDef],
}

/// Extended-thinking request configuration, sent on every turn.
///
/// `adaptive` lets the model decide when/how much to think (the only
/// accepted on-mode on the current model family — `budget_tokens` is
/// rejected with a 400 on `claude-sonnet-5` and its peers), and
/// `display: "summarized"` opts back into readable thinking text: without
/// it the API streams thinking blocks with EMPTY text (the post-4.7
/// default), which is exactly the "can't see what it's thinking" gap.
/// No settings surface by design; older pre-4.6 models that still need
/// `{type: "enabled", budget_tokens}` are out of scope here.
fn thinking_config() -> serde_json::Value {
    json!({"type": "adaptive", "display": "summarized"})
}

impl MessagesRequest<'_> {
    /// Build the request body with two `cache_control` breakpoints
    /// (Anthropic ephemeral prompt cache, default 5-minute TTL):
    ///
    /// 1. On the system prompt, which becomes the content-block form
    ///    `[{type: "text", text, cache_control}]` (omitted entirely when
    ///    empty — an empty text block is rejected by the API). The prompt
    ///    prefix hashes in request order `tools` → `system` → `messages`,
    ///    so this one marker caches the tool definitions too; no marker on
    ///    the tools array is needed.
    /// 2. On the last content block of the last transcript message — the
    ///    rolling breakpoint. Each turn's cache write covers the whole
    ///    conversation prefix before it, and the next turn's request reads
    ///    it back (prior breakpoints stay valid read points).
    ///
    /// Cache hits surface in usage as `cache_read_input_tokens`; prefixes
    /// below the model's minimum cacheable length silently don't cache.
    pub fn body(&self) -> serde_json::Value {
        let mut body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "stream": self.stream,
            "thinking": thinking_config(),
        });
        if !self.system.is_empty() {
            body["system"] = json!([{
                "type": "text",
                "text": self.system,
                "cache_control": cache_control_ephemeral(),
            }]);
        }
        let mut messages =
            serde_json::to_value(self.messages).expect("message serialization is infallible");
        if let Some(msgs) = messages.as_array_mut() {
            // Unsigned thinking blocks (compat-provider origin) never go on
            // the wire: the API validates replayed thinking against its
            // signature, and blocks it did not produce would be rejected.
            // Signed and redacted blocks pass through verbatim, as the
            // tool-use loop requires.
            for msg in msgs.iter_mut() {
                if let Some(blocks) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                    blocks
                        .retain(|block| !(block["type"] == "thinking" && block["signature"] == ""));
                }
            }
            // The rolling breakpoint skips thinking-family blocks —
            // `cache_control` is not accepted on them. In practice the last
            // message is always a user message (text or tool results), so
            // this is a guard, not the common path.
            if let Some(last_block) = msgs
                .last_mut()
                .and_then(|msg| msg.get_mut("content"))
                .and_then(|content| content.as_array_mut())
                .and_then(|blocks| {
                    blocks.iter_mut().rev().find(|block| {
                        block["type"] != "thinking" && block["type"] != "redacted_thinking"
                    })
                })
            {
                last_block["cache_control"] = cache_control_ephemeral();
            }
        }
        body["messages"] = messages;
        if !self.tools.is_empty() {
            body["tools"] =
                serde_json::to_value(self.tools).expect("tool serialization is infallible");
        }
        body
    }
}

/// The `cache_control` marker value (default 5-minute TTL).
fn cache_control_ephemeral() -> serde_json::Value {
    json!({"type": "ephemeral"})
}

/// One streamed SSE `data:` payload, tagged by `type`. Unknown event types
/// deserialize to [`SsePayload::Unknown`] and are ignored by the provider.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SsePayload {
    MessageStart {
        message: MessageStart,
    },
    ContentBlockStart {
        index: usize,
        content_block: WireContentBlockStart,
    },
    ContentBlockDelta {
        index: usize,
        delta: WireBlockDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDeltaBody,
        #[serde(default)]
        usage: Option<OutputUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: ApiError,
    },
    #[serde(other)]
    Unknown,
}

/// `message_start` payload: carries the input-token count.
#[derive(Debug, Deserialize)]
pub struct MessageStart {
    #[serde(default)]
    pub usage: Option<InputUsage>,
}

/// Input-side usage from `message_start`. `input_tokens` is the *uncached*
/// prompt remainder; the cache buckets report prompt tokens written to /
/// read from the ephemeral prompt cache this turn.
#[derive(Debug, Deserialize)]
pub struct InputUsage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

/// Cumulative usage from `message_delta`. `output_tokens` is the turn's
/// running total; the input-side fields are optional final overrides —
/// normally they arrive on `message_start`, but when the final delta
/// repeats them they win (absent fields never clobber earlier values).
#[derive(Debug, Deserialize)]
pub struct OutputUsage {
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

/// A `content_block_start` block, tagged by `type`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireContentBlockStart {
    Text {
        #[serde(default)]
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
    },
    /// An extended-thinking block; text/signature stream as deltas.
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    /// A redacted thinking block arrives complete — no deltas follow.
    RedactedThinking {
        #[serde(default)]
        data: String,
    },
    #[serde(other)]
    Unknown,
}

/// A `content_block_delta` delta, tagged by `type`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireBlockDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    /// A fragment of a thinking block's text.
    ThinkingDelta {
        thinking: String,
    },
    /// A fragment of a thinking block's signature (arrives at block end).
    SignatureDelta {
        signature: String,
    },
    #[serde(other)]
    Unknown,
}

/// `message_delta` body: carries the stop reason.
#[derive(Debug, Deserialize)]
pub struct MessageDeltaBody {
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// An `error` event payload (also the shape of non-2xx response bodies,
/// nested under `{ "error": ... }`).
#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub message: String,
}

/// Non-2xx response body: `{ "type": "error", "error": {...} }`.
#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    pub error: ApiError,
}

/// Map a wire `stop_reason` string to the provider-neutral enum.
pub fn map_stop_reason(s: &str) -> StopReason {
    match s {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::provider::model_provider::{ChatRole, ContentBlock};

    #[test]
    fn request_serializes_to_expected_json() {
        let messages = vec![
            ChatMessage::user_text("make it blue"),
            ChatMessage {
                role: ChatRole::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "Trying an edit.".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_1".into(),
                        name: "iterate".into(),
                        input: json!({"note": "blue"}),
                    },
                ],
            },
            ChatMessage {
                role: ChatRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: "{\"shader\":\"ok\"}".into(),
                    is_error: Some(false),
                }],
            },
        ];
        let tools = vec![ToolDef {
            name: "iterate".into(),
            description: "desc".into(),
            input_schema: json!({"type": "object"}),
        }];
        let req = MessagesRequest {
            model: "claude-sonnet-5",
            max_tokens: 4096,
            stream: true,
            system: "sys",
            messages: &messages,
            tools: &tools,
        };
        let got = req.body();
        // Breakpoint scheme: one marker on the (block-form) system prompt —
        // which also covers the tools rendered ahead of it — and one rolling
        // marker on the last content block of the last message.
        let expect = json!({
            "model": "claude-sonnet-5",
            "max_tokens": 4096,
            "stream": true,
            "thinking": {"type": "adaptive", "display": "summarized"},
            "system": [{"type": "text", "text": "sys",
                        "cache_control": {"type": "ephemeral"}}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "make it blue"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Trying an edit."},
                    {"type": "tool_use", "id": "tu_1", "name": "iterate", "input": {"note": "blue"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1",
                     "content": "{\"shader\":\"ok\"}", "is_error": false,
                     "cache_control": {"type": "ephemeral"}}
                ]}
            ],
            "tools": [{"name": "iterate", "description": "desc", "input_schema": {"type": "object"}}]
        });
        assert_eq!(got, expect);
    }

    #[test]
    fn empty_tools_and_empty_system_are_omitted() {
        let req = MessagesRequest {
            model: "m",
            max_tokens: 1,
            stream: true,
            system: "",
            messages: &[],
            tools: &[],
        };
        let got = req.body();
        let obj = got.as_object().expect("obj");
        assert!(obj.get("tools").is_none());
        // An empty system prompt must be omitted, not sent as an empty
        // text block (the API rejects empty text blocks).
        assert!(obj.get("system").is_none());
    }

    #[test]
    fn rolling_breakpoint_marks_only_the_final_block() {
        let messages = vec![
            ChatMessage::user_text("first"),
            ChatMessage {
                role: ChatRole::User,
                content: vec![
                    ContentBlock::Text { text: "a".into() },
                    ContentBlock::Text { text: "b".into() },
                ],
            },
        ];
        let req = MessagesRequest {
            model: "m",
            max_tokens: 1,
            stream: true,
            system: "sys",
            messages: &messages,
            tools: &[],
        };
        let got = req.body();
        // Only the very last block of the last message carries the marker.
        assert!(
            got["messages"][0]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert!(
            got["messages"][1]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            got["messages"][1]["content"][1]["cache_control"],
            json!({"type": "ephemeral"})
        );
    }

    #[test]
    fn signed_thinking_blocks_replay_and_unsigned_ones_are_dropped() {
        let messages = vec![
            ChatMessage {
                role: ChatRole::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        thinking: "Weighing colors.".into(),
                        signature: "sig_1".into(),
                    },
                    ContentBlock::RedactedThinking {
                        data: "opaque".into(),
                    },
                    // Compat-origin (unsigned) thinking never goes on the
                    // Anthropic wire.
                    ContentBlock::Thinking {
                        thinking: "compat reasoning".into(),
                        signature: String::new(),
                    },
                    ContentBlock::ToolUse {
                        id: "tu_1".into(),
                        name: "iterate".into(),
                        input: json!({}),
                    },
                ],
            },
            ChatMessage {
                role: ChatRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: "{}".into(),
                    is_error: None,
                }],
            },
        ];
        let req = MessagesRequest {
            model: "m",
            max_tokens: 1,
            stream: true,
            system: "",
            messages: &messages,
            tools: &[],
        };
        let got = req.body();
        let assistant = got["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(assistant.len(), 3, "{assistant:?}");
        assert_eq!(assistant[0]["type"], "thinking");
        assert_eq!(assistant[0]["thinking"], "Weighing colors.");
        assert_eq!(assistant[0]["signature"], "sig_1");
        assert_eq!(assistant[1]["type"], "redacted_thinking");
        assert_eq!(assistant[1]["data"], "opaque");
        assert_eq!(assistant[2]["type"], "tool_use");
    }

    #[test]
    fn rolling_breakpoint_skips_thinking_family_blocks() {
        // Defensive: were a thinking block ever last, the marker must land
        // on the nearest earlier cacheable block instead.
        let messages = vec![ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Text { text: "a".into() },
                ContentBlock::Thinking {
                    thinking: "t".into(),
                    signature: "sig".into(),
                },
                ContentBlock::RedactedThinking { data: "d".into() },
            ],
        }];
        let req = MessagesRequest {
            model: "m",
            max_tokens: 1,
            stream: true,
            system: "",
            messages: &messages,
            tools: &[],
        };
        let got = req.body();
        let blocks = got["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(
            blocks[0]["cache_control"],
            json!({"type": "ephemeral"}),
            "{blocks:?}"
        );
        assert!(blocks[1].get("cache_control").is_none());
        assert!(blocks[2].get("cache_control").is_none());
    }

    #[test]
    fn cache_usage_fields_deserialize_and_default_to_zero() {
        let usage: InputUsage = serde_json::from_str(
            r#"{"input_tokens":21,"cache_creation_input_tokens":2100,"cache_read_input_tokens":18000}"#,
        )
        .expect("parse");
        assert_eq!(usage.input_tokens, 21);
        assert_eq!(usage.cache_creation_input_tokens, 2100);
        assert_eq!(usage.cache_read_input_tokens, 18000);

        // Absent cache fields (older responses, other gateways) default.
        let usage: InputUsage = serde_json::from_str(r#"{"input_tokens":25}"#).expect("parse");
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
    }

    #[test]
    fn sse_payloads_deserialize() {
        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_1","name":"iterate","input":{}}}"#,
        )
        .expect("parse");
        match p {
            SsePayload::ContentBlockStart {
                index,
                content_block: WireContentBlockStart::ToolUse { id, name },
            } => {
                assert_eq!((index, id.as_str(), name.as_str()), (1, "tu_1", "iterate"));
            }
            other => panic!("unexpected: {other:?}"),
        }

        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"no"}}"#,
        )
        .expect("parse");
        match p {
            SsePayload::ContentBlockDelta {
                delta: WireBlockDelta::InputJsonDelta { partial_json },
                ..
            } => assert_eq!(partial_json, "{\"no"),
            other => panic!("unexpected: {other:?}"),
        }

        let p: SsePayload = serde_json::from_str(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":77}}"#,
        )
        .expect("parse");
        match p {
            SsePayload::MessageDelta { delta, usage } => {
                assert_eq!(delta.stop_reason.as_deref(), Some("tool_use"));
                assert_eq!(usage.expect("usage").output_tokens, 77);
            }
            other => panic!("unexpected: {other:?}"),
        }

        let p: SsePayload =
            serde_json::from_str(r#"{"type":"brand_new_event","stuff":1}"#).expect("parse");
        assert!(matches!(p, SsePayload::Unknown));
    }

    #[test]
    fn thinking_sse_payloads_deserialize() {
        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}"#,
        )
        .expect("parse");
        assert!(matches!(
            p,
            SsePayload::ContentBlockStart {
                content_block: WireContentBlockStart::Thinking { thinking },
                ..
            } if thinking.is_empty()
        ));

        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me look"}}"#,
        )
        .expect("parse");
        assert!(matches!(
            p,
            SsePayload::ContentBlockDelta {
                delta: WireBlockDelta::ThinkingDelta { thinking },
                ..
            } if thinking == "Let me look"
        ));

        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"EqQBCg"}}"#,
        )
        .expect("parse");
        assert!(matches!(
            p,
            SsePayload::ContentBlockDelta {
                delta: WireBlockDelta::SignatureDelta { signature },
                ..
            } if signature == "EqQBCg"
        ));

        let p: SsePayload = serde_json::from_str(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"EmwKAhgB"}}"#,
        )
        .expect("parse");
        assert!(matches!(
            p,
            SsePayload::ContentBlockStart {
                content_block: WireContentBlockStart::RedactedThinking { data },
                ..
            } if data == "EmwKAhgB"
        ));
    }

    #[test]
    fn stop_reasons_map() {
        assert_eq!(map_stop_reason("end_turn"), StopReason::EndTurn);
        assert_eq!(map_stop_reason("tool_use"), StopReason::ToolUse);
        assert_eq!(map_stop_reason("max_tokens"), StopReason::MaxTokens);
        assert_eq!(
            map_stop_reason("stop_sequence"),
            StopReason::Other("stop_sequence".into())
        );
    }
}
