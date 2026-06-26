//! Compact state summaries for pane and workflow chrome.

/// A short current-state label with a visual kind.
///
/// Use status for the chrome-level answer to "where is this surface right
/// now?" Keep it compact; put explanations in `UiViewContent` or `UiIssue`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiStatus {
    /// Short label shown in status chrome.
    pub label: String,
    /// Visual treatment for the label.
    pub kind: UiStatusKind,
}

impl UiStatus {
    /// Create a status with an explicit kind.
    pub fn new(label: impl Into<String>, kind: UiStatusKind) -> Self {
        Self {
            label: label.into(),
            kind,
        }
    }

    /// Create a neutral status for inactive or selection states.
    pub fn neutral(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Neutral)
    }

    /// Create a working status for in-progress states.
    pub fn working(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Working)
    }

    /// Create a good status for ready or successful states.
    pub fn good(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Good)
    }

    /// Create a warning status for states that need attention but can continue.
    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Warning)
    }

    /// Create an error status for failed states.
    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label, UiStatusKind::Error)
    }
}

/// Visual kind for a `UiStatus`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiStatusKind {
    /// Idle, inactive, or awaiting user choice.
    Neutral,
    /// Work is currently running.
    Working,
    /// Ready, connected, or successful.
    Good,
    /// Attention needed, but not a hard failure.
    Warning,
    /// Failed or blocked.
    Error,
}
