//! Transient feedback emitted by successful or non-fatal actions.
//!
//! Notices are action outcomes, not persistent controller state. The shell can
//! render them as logs, toasts, banners, or any other short-lived feedback
//! surface.

/// Display level for a transient notice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiNoticeLevel {
    /// Informational outcome.
    Info,
    /// Outcome that succeeded but needs attention or follow-up.
    Warning,
    /// Non-fatal error feedback emitted as an action outcome.
    Error,
}

/// A short-lived message produced by a UI action.
///
/// Use notices to report what just happened after an action finishes. Use
/// `UiIssue` instead when the problem is part of current view state, and use
/// `UiError` when the action itself failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNotice {
    /// How strongly the shell should present the notice.
    pub level: UiNoticeLevel,
    /// User-facing notice text.
    pub message: String,
}

impl UiNotice {
    /// Create an informational notice.
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: UiNoticeLevel::Info,
            message: message.into(),
        }
    }

    /// Create a warning notice for successful outcomes that still need attention.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: UiNoticeLevel::Warning,
            message: message.into(),
        }
    }

    /// Create an error notice for non-fatal action feedback.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: UiNoticeLevel::Error,
            message: message.into(),
        }
    }
}

/// The transient notices returned by a dispatched action.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiNotices {
    /// Notices emitted by the action in display order.
    pub notices: Vec<UiNotice>,
}

impl UiNotices {
    /// Create an empty notice collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a notice and return the updated collection.
    pub fn with_notice(mut self, notice: UiNotice) -> Self {
        self.notices.push(notice);
        self
    }
}
