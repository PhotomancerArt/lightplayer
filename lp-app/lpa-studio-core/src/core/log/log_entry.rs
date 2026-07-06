//! A single timestamped console log line.

use super::{UiLogLevel, UiLogSource};

/// A single timestamped log line with structured source and severity.
///
/// Use log entries for chronological diagnostic output. Unlike `UiNotice`, a
/// log entry is part of a durable stream shown in a console-like surface.
///
/// Entries are stamped by the `StudioController` at push time — producers
/// build [`UiLogDraft`](super::UiLogDraft)s (see the module docs) — so
/// `timestamp` reflects when the entry entered the ring, not when the
/// underlying event occurred on a device.
///
/// The `f64` timestamp means this type is `PartialEq` but not `Eq`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiLogEntry {
    /// Seconds since the Unix epoch; the fractional part carries sub-second
    /// precision.
    pub timestamp: f64,
    /// Severity used for visual treatment and filtering.
    pub level: UiLogLevel,
    /// Structured source: a filterable origin plus display-only detail.
    pub source: UiLogSource,
    /// The log message body.
    pub message: String,
}

impl UiLogEntry {
    /// Create a stamped log line.
    ///
    /// Only stamping code (the controller) and display fixtures/tests should
    /// call this directly; producers build
    /// [`UiLogDraft`](super::UiLogDraft)s instead.
    pub fn new(
        timestamp: f64,
        level: UiLogLevel,
        source: impl Into<UiLogSource>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp,
            level,
            source: source.into(),
            message: message.into(),
        }
    }
}
