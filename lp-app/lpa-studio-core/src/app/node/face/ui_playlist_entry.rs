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
    /// Clicking the chip selects/focuses the entry's child node — the
    /// existing node-select action, reused. Activation-by-click has no wire
    /// op today (the engine switches entries via trigger messages and timed
    /// advance only), so selection is the whole gesture. `None` when the
    /// entry's child is not mounted.
    pub action: Option<UiAction>,
}
