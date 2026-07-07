//! Unstamped log lines produced away from the clock.

use crate::core::error::UiError;
use crate::core::notice::UiNotice;

use super::{UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource};

/// An unstamped log line: everything in a [`UiLogEntry`] except the timestamp.
///
/// Producers (link providers, server clients, event mappers) cannot know the
/// wall clock — core stays platform-free — so they build drafts and the
/// `StudioController` stamps them with its injected
/// [`LogClock`](super::LogClock) at push time. Without an `f64` field a draft
/// keeps `Eq`, so producer outcome types that carry drafts stay `Eq` too.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLogDraft {
    /// Severity used for visual treatment and filtering.
    pub level: UiLogLevel,
    /// Structured source: a filterable origin plus display-only detail.
    pub source: UiLogSource,
    /// The log message body.
    pub message: String,
}

impl UiLogDraft {
    /// Create a draft with a level, source, and message.
    pub fn new(
        level: UiLogLevel,
        source: impl Into<UiLogSource>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            source: source.into(),
            message: message.into(),
        }
    }

    /// Stamp this draft into a [`UiLogEntry`] at `timestamp` (seconds since
    /// the Unix epoch).
    pub fn stamp(self, timestamp: f64) -> UiLogEntry {
        UiLogEntry {
            timestamp,
            level: self.level,
            source: self.source,
            message: self.message,
        }
    }

    /// Map a completed-action notice to a Studio-origin draft.
    ///
    /// Moved from the web crate (Q5): notice→log severity mapping is policy,
    /// not rendering, so it belongs beside the log type. The web crate keeps
    /// only the JS-console sink.
    pub fn from_notice(notice: UiNotice) -> Self {
        Self::new(
            UiLogLevel::from_notice_level(notice.level),
            UiLogOrigin::Studio,
            notice.message,
        )
    }

    /// Map an action error to a Studio-origin draft.
    ///
    /// A cancellation is informational (the user asked to stop); every other
    /// error is an `Error`. Moved from the web crate (Q5).
    pub fn from_error(error: UiError) -> Self {
        let level = if matches!(&error, UiError::Cancelled(_)) {
            UiLogLevel::Info
        } else {
            UiLogLevel::Error
        };
        Self::new(level, UiLogOrigin::Studio, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_and_error_mappers_match_retired_web_policy() {
        assert_eq!(
            UiLogDraft::from_notice(UiNotice::warning("careful")).level,
            UiLogLevel::Warn
        );
        assert_eq!(
            UiLogDraft::from_error(UiError::Cancelled("stopped".to_string())).level,
            UiLogLevel::Info
        );
        assert_eq!(
            UiLogDraft::from_error(UiError::Project("boom".to_string())).level,
            UiLogLevel::Error
        );
    }

    #[test]
    fn mappers_use_studio_origin_without_detail() {
        let draft = UiLogDraft::from_notice(UiNotice::info("done"));

        assert_eq!(draft.source, UiLogSource::new(UiLogOrigin::Studio));
    }

    #[test]
    fn stamp_carries_every_field_onto_the_entry() {
        let entry = UiLogDraft::new(
            UiLogLevel::Warn,
            UiLogSource::with_detail(UiLogOrigin::Link, "browser-serial"),
            "slow frame",
        )
        .stamp(1234.5);

        assert_eq!(entry.timestamp, 1234.5);
        assert_eq!(entry.level, UiLogLevel::Warn);
        assert_eq!(
            entry.source,
            UiLogSource::with_detail(UiLogOrigin::Link, "browser-serial")
        );
        assert_eq!(entry.message, "slow frame");
    }
}
