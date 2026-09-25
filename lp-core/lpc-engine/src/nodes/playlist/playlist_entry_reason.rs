//! Why a playlist entry is, or is not, playing.
//!
//! Device-only state on the runtime playlist (multi-pattern plan PD10,
//! vision D5). Only one entry is ever loaded; every other authored entry is
//! dormant, and this says which kind of dormant. `NodeEntryState` and the
//! wire do not carry it: Studio learns about a failed entry through the
//! playlist's own runtime status (a warning naming the entry), not through
//! a new wire field.

use alloc::string::String;

/// The per-entry reason a [`super::PlaylistNode`] keeps for every authored
/// entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaylistEntryReason {
    /// Dormant, and nothing is wrong with it: it is simply not the one
    /// playing.
    NotPlaying,
    /// Its child is loaded (the playing entry, or the one a switch is
    /// bringing in).
    Loaded,
    /// Its load or first compile failed. Timed advance and triggers skip it
    /// until the project reloads; an explicit activate tries it again.
    Failed(String),
    /// Switched off in the tour: named in the playlist's `skip` list
    /// (authored, or written by Play mode). The tour, triggers and next/prev
    /// pass it over; an explicit activate still plays it.
    Disabled,
}

impl PlaylistEntryReason {
    /// Whether the playlist may move to this entry on its own (timed
    /// advance, a trigger, or moving on after a failure).
    #[must_use]
    pub fn is_playable(&self) -> bool {
        matches!(self, Self::NotPlaying | Self::Loaded)
    }
}
