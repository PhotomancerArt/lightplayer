//! Chronological log entries surfaced by Studio UI shells.

use std::collections::VecDeque;

use crate::core::error::UiError;
use crate::core::notice::{UiNotice, UiNoticeLevel};

/// Maximum number of log entries kept in a [`LogRing`].
///
/// The cap lives in core (Q5) so every Studio shell shares one bound; the web
/// crate's private 80-entry mirror is retired in P4. The value matches that
/// retired mirror so behaviour is unchanged.
pub const LOG_RING_CAPACITY: usize = 80;

/// A bounded, chronological log buffer.
///
/// Oldest entries are dropped once the buffer exceeds [`LOG_RING_CAPACITY`], so
/// a long-running session cannot grow the log unbounded (the retired
/// `StudioController.logs: Vec` had no cap). Ordering is preserved: the front is
/// the oldest retained entry, the back the newest.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LogRing {
    entries: VecDeque<UiLogEntry>,
}

impl LogRing {
    /// An empty ring.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    /// Append one entry, evicting the oldest if the cap is exceeded.
    pub fn push(&mut self, entry: UiLogEntry) {
        self.entries.push_back(entry);
        self.trim();
    }

    /// Append many entries in order, evicting the oldest as needed.
    pub fn extend(&mut self, entries: impl IntoIterator<Item = UiLogEntry>) {
        self.entries.extend(entries);
        self.trim();
    }

    /// Number of retained entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ring holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Retained entries oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &UiLogEntry> {
        self.entries.iter()
    }

    /// Retained entries as an owned `Vec`, oldest-first.
    ///
    /// Used when building a `UiStudioView`, which still carries a plain `Vec`.
    pub fn to_vec(&self) -> Vec<UiLogEntry> {
        self.entries.iter().cloned().collect()
    }

    fn trim(&mut self) {
        while self.entries.len() > LOG_RING_CAPACITY {
            self.entries.pop_front();
        }
    }
}

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

    /// Map a completed-action notice to a `studio`-sourced log line.
    ///
    /// Moved from the web crate (Q5): notice→log severity mapping is policy, not
    /// rendering, so it belongs beside the log type. The web crate keeps only
    /// the JS-console sink.
    pub fn from_notice(notice: UiNotice) -> Self {
        Self::new(
            UiLogLevel::from_notice_level(notice.level),
            "studio",
            notice.message,
        )
    }

    /// Map an action error to a `studio`-sourced log line.
    ///
    /// A cancellation is informational (the user asked to stop); every other
    /// error is an `Error`. Moved from the web crate (Q5).
    pub fn from_error(error: UiError) -> Self {
        let level = if matches!(&error, UiError::Cancelled(_)) {
            UiLogLevel::Info
        } else {
            UiLogLevel::Error
        };
        Self::new(level, "studio", error.to_string())
    }
}

impl UiLogLevel {
    /// The log severity that corresponds to a notice severity.
    pub fn from_notice_level(level: UiNoticeLevel) -> Self {
        match level {
            UiNoticeLevel::Info => Self::Info,
            UiNoticeLevel::Warning => Self::Warn,
            UiNoticeLevel::Error => Self::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_ring_evicts_oldest_beyond_capacity() {
        let mut ring = LogRing::new();
        for i in 0..(LOG_RING_CAPACITY + 5) {
            ring.push(UiLogEntry::new(
                UiLogLevel::Info,
                "test",
                format!("entry {i}"),
            ));
        }

        assert_eq!(ring.len(), LOG_RING_CAPACITY);
        // The five oldest were evicted; the front is entry 5, the back the last.
        let entries = ring.to_vec();
        assert_eq!(entries.first().unwrap().message, "entry 5");
        assert_eq!(
            entries.last().unwrap().message,
            format!("entry {}", LOG_RING_CAPACITY + 4)
        );
    }

    #[test]
    fn log_ring_extend_preserves_order_and_cap() {
        let mut ring = LogRing::new();
        ring.extend(
            (0..(LOG_RING_CAPACITY + 3))
                .map(|i| UiLogEntry::new(UiLogLevel::Debug, "test", format!("e{i}"))),
        );

        assert_eq!(ring.len(), LOG_RING_CAPACITY);
        assert_eq!(ring.to_vec().first().unwrap().message, "e3");
    }

    #[test]
    fn notice_and_error_mappers_match_retired_web_policy() {
        assert_eq!(
            UiLogEntry::from_notice(UiNotice::warning("careful")).level,
            UiLogLevel::Warn
        );
        assert_eq!(
            UiLogEntry::from_error(UiError::Cancelled("stopped".to_string())).level,
            UiLogLevel::Info
        );
        assert_eq!(
            UiLogEntry::from_error(UiError::Project("boom".to_string())).level,
            UiLogLevel::Error
        );
    }
}
