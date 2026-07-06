//! Display-side filtering for the console ring.

use super::{UiLogEntry, UiLogLevel, UiLogOrigin};

/// The console's display filter: a minimum severity plus per-origin toggles.
///
/// Filtering is display-side only: the [`LogRing`](super::LogRing) keeps every
/// entry (up to the cap) regardless of the filter, so relaxing the filter
/// reveals already-captured history. What gets *logged* is ingestion policy
/// and lives with the producers, not here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogFilter {
    /// Entries below this severity are hidden. Defaults to
    /// [`UiLogLevel::Info`], so debug/trace chatter (heartbeats) stays out of
    /// the console until the user opts in.
    pub min_level: UiLogLevel,
    /// Per-origin enablement, indexed by `UiLogOrigin::index`. All origins are
    /// enabled by default.
    enabled_origins: [bool; UiLogOrigin::ALL.len()],
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            min_level: UiLogLevel::Info,
            enabled_origins: [true; UiLogOrigin::ALL.len()],
        }
    }
}

impl LogFilter {
    /// Whether `entry` passes the severity threshold and its origin toggle.
    pub fn matches(&self, entry: &UiLogEntry) -> bool {
        entry.level >= self.min_level && self.is_origin_enabled(entry.source.origin)
    }

    /// Whether entries from `origin` are shown.
    pub fn is_origin_enabled(&self, origin: UiLogOrigin) -> bool {
        self.enabled_origins[origin.index()]
    }

    /// Show or hide entries from `origin`.
    pub fn set_origin_enabled(&mut self, origin: UiLogOrigin, enabled: bool) {
        self.enabled_origins[origin.index()] = enabled;
    }

    /// `(origin, enabled)` for every origin in the stable
    /// [`UiLogOrigin::ALL`] order, for toolbar rendering.
    pub fn origin_states(&self) -> Vec<(UiLogOrigin, bool)> {
        UiLogOrigin::ALL
            .into_iter()
            .map(|origin| (origin, self.is_origin_enabled(origin)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter_hides_trace_and_debug_shows_info_and_up() {
        let filter = LogFilter::default();

        assert!(!filter.matches(&entry(UiLogLevel::Trace, UiLogOrigin::Studio)));
        assert!(!filter.matches(&entry(UiLogLevel::Debug, UiLogOrigin::Studio)));
        assert!(filter.matches(&entry(UiLogLevel::Info, UiLogOrigin::Studio)));
        assert!(filter.matches(&entry(UiLogLevel::Warn, UiLogOrigin::Studio)));
        assert!(filter.matches(&entry(UiLogLevel::Error, UiLogOrigin::Studio)));
    }

    #[test]
    fn trace_threshold_shows_everything() {
        let filter = LogFilter {
            min_level: UiLogLevel::Trace,
            ..LogFilter::default()
        };

        assert!(filter.matches(&entry(UiLogLevel::Trace, UiLogOrigin::Device)));
        assert!(filter.matches(&entry(UiLogLevel::Error, UiLogOrigin::Device)));
    }

    #[test]
    fn disabled_origin_hides_entries_regardless_of_level() {
        let mut filter = LogFilter::default();
        filter.set_origin_enabled(UiLogOrigin::Server, false);

        assert!(!filter.matches(&entry(UiLogLevel::Error, UiLogOrigin::Server)));
        assert!(filter.matches(&entry(UiLogLevel::Error, UiLogOrigin::Link)));

        filter.set_origin_enabled(UiLogOrigin::Server, true);
        assert!(filter.matches(&entry(UiLogLevel::Error, UiLogOrigin::Server)));
    }

    #[test]
    fn origin_states_follow_the_stable_order() {
        let mut filter = LogFilter::default();
        filter.set_origin_enabled(UiLogOrigin::Device, false);

        assert_eq!(
            filter.origin_states(),
            vec![
                (UiLogOrigin::Studio, true),
                (UiLogOrigin::Link, true),
                (UiLogOrigin::Server, true),
                (UiLogOrigin::Device, false),
            ]
        );
    }

    fn entry(level: UiLogLevel, origin: UiLogOrigin) -> UiLogEntry {
        UiLogEntry::new(0.0, level, origin, "message")
    }
}
