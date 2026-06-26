//! Chronological log entries surfaced by Studio UI shells.

/// A single log line with source and severity.
///
/// Use log entries for chronological diagnostic output. Unlike `UiNotice`, a
/// log entry is part of a durable stream that can be shown in a console-like
/// surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLogEntry {
    /// Severity used for visual treatment and filtering.
    pub level: UiLogLevel,
    /// Short subsystem label, such as `studio`, `lpa-link`, or `fw-esp32`.
    pub source: String,
    /// The log message body.
    pub message: String,
}

/// Severity for a Studio log line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiLogLevel {
    /// Verbose diagnostic output.
    Debug,
    /// Normal informational output.
    Info,
    /// Recoverable issue or attention-worthy output.
    Warn,
    /// Failed operation or serious problem.
    Error,
}

impl UiLogEntry {
    /// Create a log line with a level, source, and message.
    pub fn new(level: UiLogLevel, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            source: source.into(),
            message: message.into(),
        }
    }
}
