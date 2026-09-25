//! Where a cycling playlist is: a pure function of the playlist's clock and
//! an anchor (multi-pattern vision D13, plan A3).
//!
//! The tour's **stops** are the authored entries in key order that may play
//! ([`super::PlaylistEntryReason::is_playable`]): skipped (`Disabled`) and
//! failed entries are passed over. The idle entry is an ordinary stop (plan
//! A2).
//!
//! The **anchor** `(entry, time)` is where the tour last started counting:
//! a pick, a trigger or next/prev sets it to `(picked, now)`, so the tour
//! carries on from the pick. From there
//!
//! ```text
//! k    = floor((t - anchor_time) / step)
//! stop = the k-th stop after anchor_entry, wrapping
//! ```
//!
//! `t` is the playlist's consumed `time` — the clock, so the tour follows
//! its speed and pause exactly as palette cycling does. At `k = 0` the tour
//! is on the anchor entry itself, even when that entry is no longer a stop
//! (skipped while playing): it plays out its step, then the tour moves on.

use super::PlaylistRuntimeEntry;

/// The tour's anchor: the entry it started counting from, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlaylistTourAnchor {
    pub entry: u32,
    pub time: f32,
}

impl PlaylistTourAnchor {
    pub fn new(entry: u32, time: f32) -> Self {
        Self { entry, time }
    }

    /// Whole steps from the anchor to `time` (negative when the clock was
    /// scrubbed back before it).
    pub fn steps_at(&self, step_seconds: f32, time: f32) -> i64 {
        libm::floorf((time - self.time) / step_seconds) as i64
    }

    /// The same step phase, counted from `entry`: the anchor the tour moves
    /// to when its stops change under it (a skip or a failure), so the entry
    /// playing keeps the rest of its step instead of starting a new one.
    #[must_use]
    pub fn rebased_on(&self, entry: u32, step_seconds: f32, time: f32) -> Self {
        let steps = self.steps_at(step_seconds, time);
        Self {
            entry,
            time: self.time + steps as f32 * step_seconds,
        }
    }
}

/// The entry the tour is on at `time`, or `None` when no entry is a stop
/// (every entry skipped or failed: the playlist holds).
///
/// `entries` must be sorted by key (the runtime playlist keeps them so).
pub(super) fn tour_entry_at(
    entries: &[PlaylistRuntimeEntry],
    anchor: PlaylistTourAnchor,
    step_seconds: f32,
    time: f32,
) -> Option<u32> {
    let steps = anchor.steps_at(step_seconds, time);
    if steps == 0 {
        return Some(anchor.entry);
    }
    let stops = || entries.iter().filter(|entry| entry.reason.is_playable());
    let count = stops().count() as i64;
    if count == 0 {
        return None;
    }
    // The anchor's position among the stops: its own index when it is one;
    // when it is not, just before the first stop after it — so one step on
    // lands on that stop.
    let at_or_before = stops().filter(|entry| entry.index <= anchor.entry).count() as i64;
    let position = (at_or_before - 1 + steps).rem_euclid(count) as usize;
    stops().nth(position).map(|entry| entry.index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::playlist::PlaylistEntryReason;
    use alloc::string::String;
    use alloc::vec::Vec;

    #[test]
    fn two_second_steps_walk_the_stops_in_order_and_wrap() {
        let entries = entries(&[1, 2, 3, 4]);
        let anchor = PlaylistTourAnchor::new(1, 0.0);

        let walk: Vec<u32> = (0..=10)
            .map(|t| tour_entry_at(&entries, anchor, 2.0, t as f32).unwrap())
            .collect();

        assert_eq!(walk, [1, 1, 2, 2, 3, 3, 4, 4, 1, 1, 2]);
    }

    #[test]
    fn skipped_and_failed_entries_are_passed_over() {
        let mut entries = entries(&[1, 2, 3, 4, 5]);
        entries[1].reason = PlaylistEntryReason::Disabled;
        entries[3].reason = PlaylistEntryReason::Failed(String::from("bad glsl"));
        let anchor = PlaylistTourAnchor::new(1, 0.0);

        let walk: Vec<u32> = (0..5)
            .map(|step| tour_entry_at(&entries, anchor, 2.0, step as f32 * 2.0).unwrap())
            .collect();

        assert_eq!(walk, [1, 3, 5, 1, 3]);
    }

    #[test]
    fn an_anchor_that_is_not_a_stop_plays_out_its_step_then_moves_on() {
        let mut entries = entries(&[1, 2, 3]);
        entries[1].reason = PlaylistEntryReason::Disabled;
        let anchor = PlaylistTourAnchor::new(2, 10.0);

        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 11.0), Some(2));
        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 12.0), Some(3));
        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 14.0), Some(1));
    }

    #[test]
    fn a_clock_scrubbed_back_walks_backwards() {
        let entries = entries(&[1, 2, 3]);
        let anchor = PlaylistTourAnchor::new(1, 10.0);

        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 9.0), Some(3));
        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 7.0), Some(2));
    }

    #[test]
    fn nothing_to_tour_holds() {
        let mut entries = entries(&[1, 2]);
        for entry in &mut entries {
            entry.reason = PlaylistEntryReason::Disabled;
        }
        let anchor = PlaylistTourAnchor::new(1, 0.0);

        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 1.0), Some(1), "k = 0");
        assert_eq!(tour_entry_at(&entries, anchor, 2.0, 2.0), None);
    }

    #[test]
    fn rebasing_keeps_the_step_phase() {
        let anchor = PlaylistTourAnchor::new(1, 0.0);

        assert_eq!(
            anchor.rebased_on(3, 2.0, 5.5),
            PlaylistTourAnchor::new(3, 4.0)
        );
    }

    fn entries(keys: &[u32]) -> Vec<PlaylistRuntimeEntry> {
        keys.iter()
            .map(|&key| PlaylistRuntimeEntry::dormant(key))
            .collect()
    }
}
