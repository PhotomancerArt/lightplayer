//! The playlist card's permanent face.

use crate::UiPlaylistEntry;

/// Permanent face for a playlist node card.
///
/// The entry strip renders on top (thumbnails, per-entry durations, cue
/// tags, ACTIVE placard on the playing entry); ONE entry's real card embeds
/// below via the existing [`crate::UiNodeChild`], so editing an entry is
/// editing the child's own card. List-on-top avoids height jumps when the
/// embedded child changes.
///
/// [`Self::active`] and [`Self::selected`] are **different axes** and may
/// name different entries: `active` is the engine's playback state, while
/// `selected` is the Studio's editing focus. The embedded child follows
/// selection, falling back to active — see
/// `node_face_builder::playlist_face`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPlaylistFace {
    /// Strip entries in mirror/tree order.
    pub entries: Vec<UiPlaylistEntry>,
    /// Entries-map key of the playing entry (`PlaylistState.active_entry`),
    /// `None` when nothing is active. Drives the ACTIVE placard only.
    pub active: Option<u32>,
    /// Entries-map key of the entry whose child holds the project-wide node
    /// focus, `None` when the selection lives outside this playlist (focus
    /// is exclusive, so that is the common case). Drives the strip's
    /// selection marking and the embedded child.
    pub selected: Option<u32>,
}
