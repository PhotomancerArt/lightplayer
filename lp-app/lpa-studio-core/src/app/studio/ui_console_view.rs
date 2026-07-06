//! The console pane's slice of the studio view.

use crate::core::log::{LogFilter, LogRing, UiLogEntry, UiLogLevel, UiLogOrigin};

/// What the console renders: the entries passing the display filter plus the
/// filter state that produced them (so the toolbar can render its controls
/// and a "N hidden" affordance without reaching into the controller).
#[derive(Clone, Debug, PartialEq)]
pub struct UiConsoleView {
    /// Entries passing the filter, oldest-first.
    pub entries: Vec<UiLogEntry>,
    /// Ring entries excluded by the level/origin filter.
    pub hidden_count: usize,
    /// The filter's current severity threshold.
    pub min_level: UiLogLevel,
    /// `(origin, enabled)` for all four origins, in the stable
    /// [`UiLogOrigin::ALL`] order.
    pub origins: Vec<(UiLogOrigin, bool)>,
    /// The connected server's last-requested runtime log level, or `None`
    /// while no server is connected. Drives the toolbar's device-level
    /// selector (disabled on `None`). Optimistic: there is no wire read-back,
    /// and a device reboot silently reverts to its init default (Info).
    pub device_log_level: Option<UiLogLevel>,
}

impl UiConsoleView {
    /// An empty console with the default filter state. The web shell seeds
    /// its view signal with this (via `UiStudioView::empty`) before the actor
    /// emits its first change-gated snapshot.
    pub fn empty() -> Self {
        Self::from_ring(&LogRing::new(), &LogFilter::default())
    }

    /// Build the display slice: `ring` entries passing `filter` oldest-first,
    /// with the excluded count and the filter state for the toolbar.
    pub fn from_ring(ring: &LogRing, filter: &LogFilter) -> Self {
        let mut entries = Vec::new();
        let mut hidden_count = 0;
        for entry in ring.iter() {
            if filter.matches(entry) {
                entries.push(entry.clone());
            } else {
                hidden_count += 1;
            }
        }
        Self {
            entries,
            hidden_count,
            min_level: filter.min_level,
            origins: filter.origin_states(),
            device_log_level: None,
        }
    }

    /// Append a progressive (mid-action) entry using this view's own filter
    /// state, so the actor's live view stays consistent with the change-gated
    /// snapshot that will replace it.
    pub fn push_live(&mut self, entry: UiLogEntry) {
        if self.accepts(&entry) {
            self.entries.push(entry);
        } else {
            self.hidden_count += 1;
        }
    }

    /// The view-side mirror of [`LogFilter::matches`], evaluated against the
    /// carried filter state.
    fn accepts(&self, entry: &UiLogEntry) -> bool {
        entry.level >= self.min_level
            && self
                .origins
                .iter()
                .any(|(origin, enabled)| *origin == entry.source.origin && *enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ring_keeps_matching_entries_oldest_first_and_counts_hidden() {
        let mut ring = LogRing::new();
        ring.push(entry(1.0, UiLogLevel::Debug, UiLogOrigin::Server)); // hidden: level
        ring.push(entry(2.0, UiLogLevel::Info, UiLogOrigin::Studio));
        ring.push(entry(3.0, UiLogLevel::Error, UiLogOrigin::Device)); // hidden: origin
        ring.push(entry(4.0, UiLogLevel::Warn, UiLogOrigin::Link));
        let mut filter = LogFilter::default();
        filter.set_origin_enabled(UiLogOrigin::Device, false);

        let console = UiConsoleView::from_ring(&ring, &filter);

        assert_eq!(console.hidden_count, 2);
        assert_eq!(
            console
                .entries
                .iter()
                .map(|entry| entry.timestamp)
                .collect::<Vec<_>>(),
            vec![2.0, 4.0],
            "entries stay oldest-first"
        );
        assert_eq!(console.min_level, UiLogLevel::Info);
        assert_eq!(console.origins.len(), UiLogOrigin::ALL.len());
        assert_eq!(
            console.device_log_level, None,
            "from_ring never knows the device level; the controller fills it"
        );
    }

    #[test]
    fn empty_console_carries_the_default_filter_state() {
        let console = UiConsoleView::empty();

        assert!(console.entries.is_empty());
        assert_eq!(console.hidden_count, 0);
        assert_eq!(console.min_level, UiLogLevel::Info);
        assert!(console.origins.iter().all(|(_, enabled)| *enabled));
    }

    #[test]
    fn push_live_applies_the_carried_filter_state() {
        let mut console = UiConsoleView::empty();

        console.push_live(entry(1.0, UiLogLevel::Debug, UiLogOrigin::Studio));
        console.push_live(entry(2.0, UiLogLevel::Warn, UiLogOrigin::Studio));

        assert_eq!(console.entries.len(), 1);
        assert_eq!(console.entries[0].timestamp, 2.0);
        assert_eq!(console.hidden_count, 1);
    }

    fn entry(timestamp: f64, level: UiLogLevel, origin: UiLogOrigin) -> UiLogEntry {
        UiLogEntry::new(timestamp, level, origin, "message")
    }
}
