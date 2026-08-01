//! OpenAI Chat Completions wire types: request serialization (from the
//! provider-neutral transcript) and streamed chunk deserialization. Pure
//! serde types plus the transcript → wire conversion, no IO.
//!
//! This is the dialect Ollama, LM Studio, llama.cpp and vLLM also speak,
//! which is why the provider is "OpenAI-compatible" rather than "OpenAI".

use serde::{Deserialize, Serialize};

use crate::provider::model_provider::{ChatMessage, ChatRole, ContentBlock, StopReason, ToolDef};

/// The streaming request body for `POST {base_url}/chat/completions`.
#[derive(Debug, Serialize)]
pub struct ChatCompletionsRequest<'a> {
    pub model: &'a str,
    pub stream: bool,
    /// `include_usage` asks for the final usage chunk (some local servers
    /// ignore it; the provider tolerates absent usage).
    pub stream_options: StreamOptions,
    /// The current parameter name (OpenAI deprecated `max_tokens`).
    /// `None` omits the field, which lets each server apply its own model
    /// max — see the ceiling note on [`super::openai_compat_provider`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    pub messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<WireTool<'a>>,
}

#[derive(Debug, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

/// One wire message. A single struct with optional fields covers all four
/// roles (`system`/`user`/`assistant`/`tool`).
#[derive(Debug, PartialEq, Serialize)]
pub struct WireMessage {
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<WireToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl WireMessage {
    fn plain(role: &'static str, content: String) -> Self {
        Self {
            role,
            content: Some(content),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// An assistant tool call echoed back into the transcript.
#[derive(Debug, PartialEq, Serialize)]
pub struct WireToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: WireFunctionCall,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct WireFunctionCall {
    pub name: String,
    /// The input JSON as a string (the wire form; not nested JSON).
    pub arguments: String,
}

/// A tool offered to the model: `{type: "function", function: {...}}`.
#[derive(Debug, Serialize)]
pub struct WireTool<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: WireFunctionDef<'a>,
}

#[derive(Debug, Serialize)]
pub struct WireFunctionDef<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub parameters: &'a serde_json::Value,
}

impl<'a> WireTool<'a> {
    pub fn from_def(def: &'a ToolDef) -> Self {
        Self {
            kind: "function",
            function: WireFunctionDef {
                name: &def.name,
                description: &def.description,
                parameters: &def.input_schema,
            },
        }
    }
}

/// Convert the neutral transcript to wire messages. The system prompt
/// becomes the leading `system` message. Assistant messages carry joined
/// text plus `tool_calls`; each neutral `ToolResult` block becomes its own
/// `tool` role message (emitted before any sibling text so tool results
/// stay adjacent to the assistant `tool_calls` message, as the protocol
/// requires — the session never interleaves text before tool results).
pub fn wire_messages(system: &str, messages: &[ChatMessage]) -> Vec<WireMessage> {
    let mut out = Vec::new();
    if !system.is_empty() {
        out.push(WireMessage::plain("system", system.to_string()));
    }
    for message in messages {
        match message.role {
            ChatRole::Assistant => out.push(assistant_message(message)),
            ChatRole::User => user_messages(message, &mut out),
        }
    }
    out
}

fn assistant_message(message: &ChatMessage) -> WireMessage {
    let mut text_parts: Vec<&str> = Vec::new();
    let mut tool_calls = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => text_parts.push(text),
            ContentBlock::ToolUse { id, name, input } => tool_calls.push(WireToolCall {
                id: id.clone(),
                kind: "function",
                function: WireFunctionCall {
                    name: name.clone(),
                    arguments: input.to_string(),
                },
            }),
            // Tool results never appear in assistant messages, and thinking
            // blocks are never echoed back on this dialect (DeepSeek et al.
            // explicitly reject replayed reasoning_content).
            ContentBlock::ToolResult { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::RedactedThinking { .. } => {}
        }
    }
    WireMessage {
        role: "assistant",
        content: (!text_parts.is_empty()).then(|| text_parts.join("\n\n")),
        tool_calls,
        tool_call_id: None,
    }
}

fn user_messages(message: &ChatMessage, out: &mut Vec<WireMessage>) {
    let mut text_parts: Vec<&str> = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => text_parts.push(text),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => out.push(WireMessage {
                role: "tool",
                content: Some(content.clone()),
                tool_calls: Vec::new(),
                tool_call_id: Some(tool_use_id.clone()),
            }),
            // Tool calls and thinking never appear in user messages.
            ContentBlock::ToolUse { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::RedactedThinking { .. } => {}
        }
    }
    if !text_parts.is_empty() {
        out.push(WireMessage::plain("user", text_parts.join("\n\n")));
    }
}

/// One streamed `data:` chunk. Both the delta chunks and the trailing
/// usage-only chunk (empty `choices`) deserialize to this.
#[derive(Debug, Default, Deserialize)]
pub struct StreamChunk {
    #[serde(default)]
    pub choices: Vec<WireChoice>,
    #[serde(default)]
    pub usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
pub struct WireChoice {
    #[serde(default)]
    pub delta: WireDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireDelta {
    #[serde(default)]
    pub content: Option<String>,
    /// Streamed reasoning text, the DeepSeek/vLLM/Ollama convention.
    #[serde(default)]
    pub reasoning_content: Option<String>,
    /// Alternate reasoning field name some servers use (e.g. OpenRouter).
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<WireToolCallDelta>,
}

impl WireDelta {
    /// The reasoning fragment, whichever field the server used
    /// (`reasoning_content` wins when both are present).
    pub fn reasoning_fragment(&self) -> Option<&str> {
        self.reasoning_content
            .as_deref()
            .or(self.reasoning.as_deref())
            .filter(|text| !text.is_empty())
    }
}

/// One tool-call fragment: `id` and `function.name` arrive on the first
/// fragment for an `index`; `function.arguments` accumulates across
/// fragments.
#[derive(Debug, Deserialize)]
pub struct WireToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: WireFunctionDelta,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// The final chunk's usage block (absent on servers that ignore
/// `stream_options.include_usage`). Note `prompt_tokens` INCLUDES any
/// cached tokens on this dialect — the provider subtracts
/// [`PromptTokensDetails::cached_tokens`] to keep the neutral usage
/// buckets disjoint.
#[derive(Debug, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// OpenAI's cached-prompt breakdown (absent on most local servers).
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
}

impl WireUsage {
    /// Cached prompt tokens, when the server reports them.
    pub fn cached_tokens(&self) -> u32 {
        self.prompt_tokens_details
            .as_ref()
            .map_or(0, |details| details.cached_tokens)
    }
}

/// `usage.prompt_tokens_details`: the cached share of `prompt_tokens`.
#[derive(Debug, Deserialize)]
pub struct PromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: u32,
}

/// Non-2xx response body: `{ "error": { "message", "type", ... } }`.
#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    pub error: ApiError,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub message: String,
}

/// Map a `finish_reason` to the provider-neutral enum.
pub fn map_finish_reason(s: &str) -> StopReason {
    match s {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
                        id: "call_1".into(),
                        name: "iterate".into(),
                        input: json!({"note": "blue"}),
                    },
                ],
            },
            ChatMessage {
                role: ChatRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".into(),
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
        let req = ChatCompletionsRequest {
            model: "gpt-test",
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
            max_completion_tokens: Some(4096),
            messages: wire_messages("sys", &messages),
            tools: tools.iter().map(WireTool::from_def).collect(),
        };
        let got = serde_json::to_value(&req).expect("serialize");
        let expect = json!({
            "model": "gpt-test",
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_completion_tokens": 4096,
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "make it blue"},
                {"role": "assistant", "content": "Trying an edit.", "tool_calls": [
                    {"id": "call_1", "type": "function",
                     "function": {"name": "iterate", "arguments": "{\"note\":\"blue\"}"}}
                ]},
                {"role": "tool", "content": "{\"shader\":\"ok\"}", "tool_call_id": "call_1"}
            ],
            "tools": [{"type": "function", "function": {
                "name": "iterate", "description": "desc", "parameters": {"type": "object"}
            }}]
        });
        assert_eq!(got, expect);
    }

    #[test]
    fn empty_tools_and_tool_call_free_messages_omit_their_fields() {
        let req = ChatCompletionsRequest {
            model: "m",
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
            max_completion_tokens: None,
            messages: wire_messages("", &[ChatMessage::user_text("hi")]),
            tools: Vec::new(),
        };
        let got = serde_json::to_value(&req).expect("serialize");
        let obj = got.as_object().expect("obj");
        assert!(obj.get("tools").is_none());
        // `None` omits the ceiling entirely rather than sending a null or a
        // zero, either of which servers would treat as a real limit.
        assert!(obj.get("max_completion_tokens").is_none());
        // No system message when the system prompt is empty.
        assert_eq!(got["messages"], json!([{"role": "user", "content": "hi"}]));
    }

    #[test]
    fn tool_call_only_assistant_message_omits_content() {
        let messages = vec![ChatMessage {
            role: ChatRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "iterate".into(),
                input: json!({}),
            }],
        }];
        let wire = wire_messages("", &messages);
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].role, "assistant");
        assert_eq!(wire[0].content, None);
        assert_eq!(wire[0].tool_calls.len(), 1);
        let json = serde_json::to_value(&wire[0]).expect("serialize");
        assert!(json.as_object().expect("obj").get("content").is_none());
    }

    #[test]
    fn stream_chunks_deserialize() {
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[{"delta":{"content":"Hel"},"finish_reason":null}]}"#,
        )
        .expect("parse");
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hel"));

        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"iterate","arguments":""}}]},"finish_reason":null}]}"#,
        )
        .expect("parse");
        let call = &chunk.choices[0].delta.tool_calls[0];
        assert_eq!(call.index, 0);
        assert_eq!(call.id.as_deref(), Some("call_1"));
        assert_eq!(call.function.name.as_deref(), Some("iterate"));

        // The trailing usage-only chunk has empty choices.
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":25,"completion_tokens":12}}"#,
        )
        .expect("parse");
        let usage = chunk.usage.expect("usage");
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (25, 12));
        // No details block → zero cached tokens.
        assert_eq!(usage.cached_tokens(), 0);
    }

    #[test]
    fn reasoning_deltas_deserialize_under_either_field_name() {
        // DeepSeek/vLLM/Ollama convention.
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[{"delta":{"reasoning_content":"Hmm, "},"finish_reason":null}]}"#,
        )
        .expect("parse");
        assert_eq!(
            chunk.choices[0].delta.reasoning_fragment(),
            Some("Hmm, "),
            "reasoning_content"
        );

        // OpenRouter-style `reasoning`.
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[{"delta":{"reasoning":"weighing options"},"finish_reason":null}]}"#,
        )
        .expect("parse");
        assert_eq!(
            chunk.choices[0].delta.reasoning_fragment(),
            Some("weighing options")
        );

        // Empty fragments and plain content chunks produce nothing.
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[{"delta":{"reasoning_content":"","content":"Hi"},"finish_reason":null}]}"#,
        )
        .expect("parse");
        assert_eq!(chunk.choices[0].delta.reasoning_fragment(), None);
    }

    #[test]
    fn thinking_blocks_never_reach_the_compat_wire() {
        let messages = vec![ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "reasoning".into(),
                    signature: String::new(),
                },
                ContentBlock::RedactedThinking { data: "x".into() },
                ContentBlock::Text {
                    text: "Answer.".into(),
                },
            ],
        }];
        let wire = wire_messages("", &messages);
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].content.as_deref(), Some("Answer."));
    }

    #[test]
    fn cached_prompt_tokens_deserialize() {
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":1000,"completion_tokens":12,
                "prompt_tokens_details":{"cached_tokens":768}}}"#,
        )
        .expect("parse");
        assert_eq!(chunk.usage.expect("usage").cached_tokens(), 768);

        // An empty details object defaults its fields.
        let chunk: StreamChunk = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":1,
                "prompt_tokens_details":{}}}"#,
        )
        .expect("parse");
        assert_eq!(chunk.usage.expect("usage").cached_tokens(), 0);
    }

    #[test]
    fn finish_reasons_map() {
        assert_eq!(map_finish_reason("stop"), StopReason::EndTurn);
        assert_eq!(map_finish_reason("tool_calls"), StopReason::ToolUse);
        assert_eq!(map_finish_reason("length"), StopReason::MaxTokens);
        assert_eq!(
            map_finish_reason("content_filter"),
            StopReason::Other("content_filter".into())
        );
    }
}
