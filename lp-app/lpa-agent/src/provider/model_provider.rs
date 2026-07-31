//! [`ModelProvider`]: the provider-neutral one-turn streaming contract.
//!
//! This trait is deliberately small: a future in-web local model must be
//! implementable against it (that constraint is the reason it exists). The
//! caller sends the full transcript every turn; the provider streams
//! [`TurnEvent`]s until the turn completes. Futures/streams are
//! runtime-neutral — no executor assumptions, no spawning.

use core::pin::Pin;

use futures_util::Stream;
use serde::{Deserialize, Serialize};

/// A boxed, runtime-neutral event stream. Not `Send`: wasm transport futures
/// are `!Send`, and every driver (Studio wasm main thread, host `block_on`
/// tests/evals) polls locally.
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + 'a>>;

/// One model turn: full transcript in, streamed [`TurnEvent`]s out.
pub trait ModelProvider {
    /// Run one model turn. The stream ends after [`TurnEvent::TurnDone`] or
    /// [`TurnEvent::ProviderError`]. Dropping the stream cancels the turn.
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent>;
}

/// Boxed providers are providers too, so embedders (Studio) can store a
/// runtime-chosen provider behind `Box<dyn ModelProvider>` while
/// [`crate::session::agent_session::AgentSession`] stays generic.
impl ModelProvider for Box<dyn ModelProvider> {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        (**self).run_turn(req)
    }
}

/// Everything a provider needs for one turn. The per-turn output-token
/// ceiling is provider-owned (each provider knows what its API and servers
/// accept — Anthropic sends `ANTHROPIC_MAX_OUTPUT_TOKENS`, while the compat
/// dialect sends none and defers to each server's model max), so no budget
/// rides the request.
#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDef>,
}

/// Streamed turn progress.
#[derive(Clone, Debug, PartialEq)]
pub enum TurnEvent {
    /// A fragment of assistant text.
    TextDelta(String),
    /// A fragment of the model's thinking/reasoning text (Anthropic
    /// `thinking_delta`, OpenAI-compat `reasoning_content`/`reasoning`).
    ThinkingDelta(String),
    /// A fragment of the Anthropic thinking-block signature. Signatures
    /// close a thinking block and must be replayed with it verbatim;
    /// compat providers never emit this.
    ThinkingSignature(String),
    /// A complete opaque `redacted_thinking` block (Anthropic). Passed
    /// through the transcript verbatim, never displayed.
    RedactedThinking(String),
    /// The model started a tool call; input JSON follows as deltas.
    ToolUseStart { id: String, name: String },
    /// A fragment of the tool call's input JSON. The consumer accumulates
    /// fragments per `id` and parses the JSON once the turn completes.
    ToolInputDelta { id: String, json_fragment: String },
    /// The turn completed normally.
    TurnDone {
        stop_reason: StopReason,
        usage: TokenUsage,
    },
    /// The turn failed. `retryable` marks transient failures (rate limit,
    /// overload, network); the provider has already applied its own
    /// single-retry policy before surfacing this.
    ProviderError { message: String, retryable: bool },
}

/// Why the model stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// Natural end of the assistant message.
    EndTurn,
    /// The model wants its tool calls executed.
    ToolUse,
    /// The `max_tokens` budget cut the message short.
    MaxTokens,
    /// Any other provider-reported reason, verbatim.
    Other(String),
}

/// Token usage for one turn (or a running total). The four buckets are
/// disjoint: `input_tokens` is the *uncached* prompt remainder (Anthropic
/// semantics — the full prompt size is the sum of the three input-side
/// buckets), `cache_write_tokens` were written to the provider prompt
/// cache this turn, and `cache_read_tokens` were served from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Prompt tokens written to the provider cache (billed at a premium).
    #[serde(default)]
    pub cache_write_tokens: u32,
    /// Prompt tokens read from the provider cache (billed at a discount).
    #[serde(default)]
    pub cache_read_tokens: u32,
}

impl TokenUsage {
    /// Accumulate another turn's usage into this total.
    pub fn add(&mut self, other: TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }
}

/// A tool offered to the model. `input_schema` is JSON Schema; the shape
/// matches the Anthropic wire form (`{ name, description, input_schema }`)
/// but is provider-neutral.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// One transcript message. The serde form doubles as the Anthropic wire
/// shape (roles + typed content blocks); it is generic enough for other
/// providers to map from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Vec<ContentBlock>,
}

impl ChatMessage {
    /// A plain-text user message.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

/// Message author.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

/// One typed content block within a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Assistant or user text.
    Text { text: String },
    /// An assistant tool call (recorded back into the transcript).
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool result, sent in a `user` message.
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        is_error: Option<bool>,
    },
    /// An assistant thinking block. Anthropic requires these replayed in
    /// the transcript exactly as received (text + signature) when the
    /// session loops back with tool results; the signature is the API's
    /// integrity proof. Compat-origin thinking has an empty signature and
    /// is never sent back to any provider.
    Thinking { thinking: String, signature: String },
    /// An opaque redacted thinking block (Anthropic), passed through the
    /// transcript verbatim.
    RedactedThinking { data: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_blocks_serialize_to_wire_shapes() {
        let msg = ChatMessage {
            role: ChatRole::User,
            content: vec![
                ContentBlock::Text { text: "hi".into() },
                ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: "{}".into(),
                    is_error: None,
                },
            ],
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][1]["type"], "tool_result");
        assert_eq!(json["content"][1]["tool_use_id"], "tu_1");
        // `is_error: None` must be absent, not null.
        assert!(
            json["content"][1]
                .as_object()
                .expect("obj")
                .get("is_error")
                .is_none(),
            "{json}"
        );
    }

    #[test]
    fn thinking_blocks_serialize_to_wire_shapes() {
        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "Considering options.".into(),
                    signature: "sig_abc".into(),
                },
                ContentBlock::RedactedThinking {
                    data: "opaque".into(),
                },
                ContentBlock::Text { text: "Hi".into() },
            ],
        };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["content"][0]["type"], "thinking");
        assert_eq!(json["content"][0]["thinking"], "Considering options.");
        assert_eq!(json["content"][0]["signature"], "sig_abc");
        assert_eq!(json["content"][1]["type"], "redacted_thinking");
        assert_eq!(json["content"][1]["data"], "opaque");
        // And the round trip preserves them exactly (the replay guarantee).
        let back: ChatMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, msg);
    }

    #[test]
    fn token_usage_accumulates() {
        let mut total = TokenUsage::default();
        total.add(TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_write_tokens: 100,
            cache_read_tokens: 0,
        });
        total.add(TokenUsage {
            input_tokens: 3,
            output_tokens: 7,
            cache_write_tokens: 20,
            cache_read_tokens: 90,
        });
        assert_eq!(
            total,
            TokenUsage {
                input_tokens: 13,
                output_tokens: 12,
                cache_write_tokens: 120,
                cache_read_tokens: 90,
            }
        );
    }
}
