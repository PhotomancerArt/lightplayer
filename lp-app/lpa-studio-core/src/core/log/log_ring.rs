//! The bounded, chronological log buffer behind the console.

use std::collections::VecDeque;

use super::UiLogEntry;

/// Maximum number of log entries kept in a [`LogRing`].
///
/// The cap lives in core (Q5) so every Studio shell shares one bound. 1000
/// entries gives the console useful scrollback (device boot output plus a
/// session of heartbeats) while bounding memory; the display-side
/// [`LogFilter`](super::LogFilter) keeps the rendered list short.
pub const LOG_RING_CAPACITY: usize = 1000;

/// A bounded, chronological log buffer.
///
/// Oldest entries are dropped once the buffer exceeds [`LOG_RING_CAPACITY`],
/// so a long-running session cannot grow the log unbounded (the retired
/// `StudioController.logs: Vec` had no cap). Ordering is preserved: the front
/// is the oldest retained entry, the back the newest.
#[derive(Clone, Debug, Default, PartialEq)]
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

    /// Remove every entry (the console's Clear command).
    pub fn clear(&mut self) {
        self.entries.clear();
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
    /// Used when building snapshots, which carry a plain `Vec`.
    pub fn to_vec(&self) -> Vec<UiLogEntry> {
        self.entries.iter().cloned().collect()
    }

    fn trim(&mut self) {
        while self.entries.len() > LOG_RING_CAPACITY {
            self.entries.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{UiLogLevel, UiLogOrigin};
    use super::*;

    #[test]
    fn log_ring_evicts_oldest_beyond_capacity() {
        let mut ring = LogRing::new();
        for i in 0..(LOG_RING_CAPACITY + 5) {
            ring.push(test_entry(&format!("entry {i}")));
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
        ring.extend((0..(LOG_RING_CAPACITY + 3)).map(|i| test_entry(&format!("e{i}"))));

        assert_eq!(ring.len(), LOG_RING_CAPACITY);
        assert_eq!(ring.to_vec().first().unwrap().message, "e3");
    }

    #[test]
    fn log_ring_clear_empties_the_buffer() {
        let mut ring = LogRing::new();
        ring.push(test_entry("one"));
        ring.push(test_entry("two"));

        ring.clear();

        assert!(ring.is_empty());
    }

    fn test_entry(message: &str) -> UiLogEntry {
        UiLogEntry::new(0.0, UiLogLevel::Info, UiLogOrigin::Studio, message)
    }
}
