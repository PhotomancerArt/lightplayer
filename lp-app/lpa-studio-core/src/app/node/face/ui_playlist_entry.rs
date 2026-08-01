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
    /// Clicking a non-active chip activates the entry NOW — a
    /// `PlaylistActivateOp` runtime poke through the wire command channel
    /// (`docs/adr/2026-07-27-runtime-node-command-channel.md`): nothing is
    /// staged in the overlay, and the ACTIVE placard follows via ordinary
    /// reads. Present for every non-active entry, mounted child or not.
    /// The ACTIVE entry's chip instead carries the child select/Focus
    /// action (activating what already plays is a no-op), `None` when its
    /// child is not mounted.
    pub action: Option<UiAction>,
}
