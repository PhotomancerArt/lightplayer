//! The playlist card's permanent face.

use crate::UiPlaylistEntry;

/// Permanent face for a playlist node card.
///
/// The entry strip renders on top (thumbnails, per-entry durations, cue
/// tags, ACTIVE placard on the playing entry); the active child's real card
/// embeds below via the existing [`crate::UiNodeChild`], so editing the
/// active entry is editing the child's own card. List-on-top avoids height
/// jumps when the active entry changes.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPlaylistFace {
    /// Strip entries in mirror/tree order.
    pub entries: Vec<UiPlaylistEntry>,
    /// Entries-map key of the playing entry (`PlaylistState.active_entry`),
    /// `None` when nothing is active.
    pub active: Option<u32>,
}
