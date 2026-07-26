//! Anthropic `/v1/messages` wire types: request serialization and streamed
//! SSE payload deserialization. Pure serde types, no IO.

use serde::{Deserialize, Serialize};

use crate::provider::model_provider::{ChatMessage, StopReason, ToolDef};

/// The streaming request body. Transcript [`ChatMessage`]s and [`ToolDef`]s
/// serialize directly to the wire shapes (`{ role, content: [...] }`,
/// `{ name, description, input_schema }`).
#[derive(Debug, Serialize)]
pub struct MessagesRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub stream: bool,
    pub system: &'a str,
    pub messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "<[ToolDef]>::is_empty")]
    pub tools: &'a [ToolDef],
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

#[derive(Debug, Deserialize)]
pub struct InputUsage {
    #[serde(default)]
    pub input_tokens: u32,
}

/// Cumulative output usage from `message_delta`.
#[derive(Debug, Deserialize)]
pub struct OutputUsage {
    #[serde(default)]
    pub output_tokens: u32,
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
        let got = serde_json::to_value(&req).expect("serialize");
        let expect = json!({
            "model": "claude-sonnet-5",
            "max_tokens": 4096,
            "stream": true,
            "system": "sys",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "make it blue"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Trying an edit."},
                    {"type": "tool_use", "id": "tu_1", "name": "iterate", "input": {"note": "blue"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1",
                     "content": "{\"shader\":\"ok\"}", "is_error": false}
                ]}
            ],
            "tools": [{"name": "iterate", "description": "desc", "input_schema": {"type": "object"}}]
        });
        assert_eq!(got, expect);
    }

    #[test]
    fn empty_tools_are_omitted() {
        let req = MessagesRequest {
            model: "m",
            max_tokens: 1,
            stream: true,
            system: "",
            messages: &[],
            tools: &[],
        };
        let got = serde_json::to_value(&req).expect("serialize");
        assert!(got.as_object().expect("obj").get("tools").is_none());
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
