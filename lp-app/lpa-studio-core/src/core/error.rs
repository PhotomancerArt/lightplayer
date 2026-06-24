//! Result and error types for UI-controller operations.
//!
//! Errors in this module describe failed operations. They are control-flow for
//! commands and actions, not persistent view state. Use `UiIssue` when a
//! problem should remain visible inside a pane until the controller state
//! changes.

use core::fmt;

/// Standard result returned by a dispatched UI action.
///
/// Successful actions may return transient `UiNotice` values for logs, toasts,
/// or other shell-level feedback. Failed actions return a `UiError`.
pub type UiResult = Result<crate::UiNotices, UiError>;

/// A failed UI or controller operation.
///
/// Use this for operations that could not complete: missing sessions,
/// unsupported actions, transport/protocol failures, and user cancellation. A
/// renderer may turn a `UiError` into a log line, but controllers should map it
/// into `UiIssue` if the problem needs to be part of the current view state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiError {
    /// The current build or runtime cannot provide the requested feature.
    UnsupportedFeature(String),
    /// The action could not be routed or did not contain the expected op.
    UnsupportedAction(String),
    /// The action requires a connection/session that is not currently present.
    MissingSession(String),
    /// A link provider or link session failed.
    Link(String),
    /// A project operation failed.
    Project(String),
    /// A transport-level read/write operation failed.
    Transport(String),
    /// The server protocol returned an unexpected or invalid response.
    Protocol(String),
    /// A browser API failed or was unavailable.
    Browser(String),
    /// The user cancelled an operation, such as browser serial port selection.
    Cancelled(String),
    /// The device is reachable but is not currently running LightPlayer.
    NoFirmwareDetected(String),
}

impl UiError {
    /// Return the user-facing message carried by the error.
    pub fn message(&self) -> &str {
        match self {
            Self::UnsupportedFeature(message)
            | Self::UnsupportedAction(message)
            | Self::MissingSession(message)
            | Self::Link(message)
            | Self::Project(message)
            | Self::Transport(message)
            | Self::Protocol(message)
            | Self::Browser(message)
            | Self::Cancelled(message)
            | Self::NoFirmwareDetected(message) => message,
        }
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFeature(message) => write!(f, "unsupported feature: {message}"),
            Self::UnsupportedAction(message) => write!(f, "unsupported action: {message}"),
            Self::MissingSession(message) => write!(f, "missing session: {message}"),
            Self::Link(message) => write!(f, "link error: {message}"),
            Self::Project(message) => write!(f, "project error: {message}"),
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::Browser(message) => write!(f, "browser error: {message}"),
            Self::Cancelled(message) => f.write_str(message),
            Self::NoFirmwareDetected(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for UiError {}
