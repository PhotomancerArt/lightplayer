//! [`AgentEvent`]: what the UI renders while a session run progresses.
//!
//! Mirrors [`TurnEvent`](crate::provider::model_provider::TurnEvent) plus
//! the session-level events (tool execution, limits, completion).

use serde_json::Value;

use crate::provider::model_provider::{StopReason, TokenUsage};
use crate::tool::tool_phase::ToolPhase;

/// One UI-facing session event.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    /// A fragment of assistant text.
    TextDelta(String),
    /// The model started a tool call.
    ToolUseStart { id: String, name: String },
    /// A fragment of the tool call's input JSON (for live UI display).
    ToolInputDelta { id: String, json_fragment: String },
    /// The tool call's input JSON finished accumulating (pre-execution);
    /// `note` is the model's one-line intent, when it sent one. The full
    /// input stays in the transcript — the UI row only needs the note.
    ToolInputReady { id: String, note: Option<String> },
    /// The executing tool entered a new phase (live tool-row activity).
    ToolProgress { id: String, phase: ToolPhase },
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
