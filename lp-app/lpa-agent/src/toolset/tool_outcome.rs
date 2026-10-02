//! [`ToolOutcome`]: one executed tool call, ready for the transcript and
//! the UI.

use serde_json::Value;

/// Result of one tool call.
#[derive(Clone, Debug)]
pub struct ToolOutcome {
    /// JSON text for the `tool_result` content.
    pub content: String,
    /// True only for host/internal failures (never for a rejection the
    /// model can act on — those ride `content`, in-band).
    pub is_error: bool,
    /// Compact JSON for the UI's tool row.
    pub summary: Value,
}
