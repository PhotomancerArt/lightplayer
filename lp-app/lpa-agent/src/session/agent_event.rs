//! [`AgentEvent`]: what the UI renders while a session run progresses.
//!
//! Mirrors [`TurnEvent`](crate::provider::model_provider::TurnEvent) plus
//! the session-level events (tool execution, limits, completion).

use serde_json::Value;

use crate::provider::model_provider::{StopReason, TokenUsage};

/// One UI-facing session event.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    /// A fragment of assistant text.
    TextDelta(String),
    /// The model started a tool call.
    ToolUseStart { id: String, name: String },
    /// A fragment of the tool call's input JSON (for live UI display).
    ToolInputDelta { id: String, json_fragment: String },
    /// A tool call finished executing; `summary_json` is the compact result
    /// summary for the tool row.
    ToolExecuted {
        id: String,
        name: String,
        summary_json: Value,
    },
    /// One model turn (API call) completed.
    TurnDone {
        stop_reason: StopReason,
        usage: TokenUsage,
    },
    /// The per-run turn limit was hit; the run stopped cleanly.
    MaxTurnsReached { turns: u32 },
    /// The run was aborted via the abort handle.
    Aborted,
    /// The provider failed; the run stops after this event.
    ProviderError { message: String, retryable: bool },
    /// The run is over (normal end, limit, or abort).
    SessionDone { usage_total: TokenUsage },
}
