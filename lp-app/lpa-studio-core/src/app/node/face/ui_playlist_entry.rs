//! One entry chip in the playlist face's strip.

use crate::{UiAction, UiProductPreview};

/// A playlist entry as the face strip renders it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPlaylistEntry {
    /// Stable entries-map key (matches `PlaylistState.active_entry`).
    pub key: u32,
    /// Entry display name (authored name or the child node's label).
    pub name: String,
    /// Authored per-entry duration, when the entry auto-advances.
    pub duration_ms: Option<u64>,
    /// True when the entry is trigger-driven (authored `trigger_ids`
    /// non-empty) — rendered as a cue tag instead of a duration.
    pub cue: bool,
    /// Thumbnail preview for the entry's child output, `None` before any
    /// probe lands.
    pub thumb: Option<UiProductPreview>,
    /// The ACTIVE entry's chip: select/Focus its mounted child (a view
    /// gesture — activating what already plays is a no-op), `None` for
    /// every other entry and when the active child is not mounted. A
    /// non-active chip presses the playlist's `play` offer
    /// (`project/<playlist>/play`, `entry` = [`Self::key`]): a
    /// `PlaylistActivateOp` runtime poke through the wire command channel
    /// (`docs/adr/2026-07-27-runtime-node-command-channel.md`), nothing
    /// staged in the overlay.
    ///
    /// Still a core-built action on a DTO field: focus is a view gesture,
    /// which stays web-local navigation until the roadmap's M9 decides how
    /// view gestures are offered.
    pub focus: Option<UiAction>,
}
