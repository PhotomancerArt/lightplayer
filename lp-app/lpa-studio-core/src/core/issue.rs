//! Inline problems shown as part of the current UI state.
//!
//! Issues differ from `UiError`: an error is returned from a failed operation,
//! while an issue is stored in controller/view state until the user retries,
//! changes input, or the controller reaches a resolved state.

/// A persistent, user-visible problem inside a pane or view body.
///
/// Use an issue when the UI should explain what needs attention in the current
/// surface, such as a failed project sync or unavailable link provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiIssue {
    /// Short problem summary suitable for inline display.
    pub message: String,
    /// Optional detail with recovery hints or lower-level context.
    pub detail: Option<String>,
}

impl UiIssue {
    /// Create an issue with a summary message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: None,
        }
    }

    /// Attach supporting detail to the issue.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
