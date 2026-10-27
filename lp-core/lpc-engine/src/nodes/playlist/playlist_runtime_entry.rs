//! One authored playlist entry, as the runtime playlist sees it, and the
//! order the playlist walks them in.
//!
//! The runtime playlist holds EVERY authored entry (plan PD4), loaded or
//! not. Only the resident one has a child; the engine fills and clears it
//! through the playlist's `entry_loaded` / `entry_unloaded` hooks.

use alloc::vec::Vec;

use lpc_model::{NodeId, SlotPath};

use super::PlaylistEntryReason;

/// An authored playlist entry and its runtime residency.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistRuntimeEntry {
    /// The entry's authored key.
    pub index: u32,
    /// The entry's child node while it is loaded; `None` while dormant.
    pub child: Option<NodeId>,
    /// The child's visual output slot (always `output` today).
    pub output_slot: SlotPath,
    pub duration: Option<f32>,
    pub fade_after: Option<f32>,
    /// Trigger message ids that start or restart this entry; `None` means the
    /// entry is never triggered.
    pub trigger_ids: Option<Vec<u32>>,
    /// Why the entry is or is not playing (device-only, plan PD10).
    pub reason: PlaylistEntryReason,
}

impl PlaylistRuntimeEntry {
    /// A dormant entry: no child, not playing.
    pub fn dormant(index: u32) -> Self {
        Self {
            index,
            child: None,
            output_slot: SlotPath::parse("output").expect("playlist child output path"),
            duration: None,
            fade_after: None,
            trigger_ids: None,
            reason: PlaylistEntryReason::NotPlaying,
        }
    }

    /// The same entry, loaded as `child`.
    #[must_use]
    pub fn loaded(mut self, child: NodeId) -> Self {
        self.child = Some(child);
        self.reason = PlaylistEntryReason::Loaded;
        self
    }
}

/// The next playable entry after `key`, wrapping: one step of the shared
/// next/prev rule ([`lpc_model::playlist_step`]).
pub(super) fn next_playable_after(entries: &[PlaylistRuntimeEntry], key: u32) -> Option<u32> {
    let keys: Vec<u32> = entries.iter().map(|entry| entry.index).collect();
    lpc_model::playlist_step(&keys, key, 1, |candidate| {
        entries
            .iter()
            .any(|entry| entry.index == candidate && entry.reason.is_playable())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn next_playable_after_skips_failed_entries_and_wraps() {
        let mut entries = entries(&[1, 2, 3, 4]);
        entries[2].reason = PlaylistEntryReason::Failed(String::from("bad glsl"));

        assert_eq!(next_playable_after(&entries, 2), Some(4), "3 failed");
        assert_eq!(next_playable_after(&entries, 4), Some(1), "wraps round");
        assert_eq!(next_playable_after(&entries, 3), Some(4));
    }

    #[test]
    fn next_playable_after_is_none_when_everything_else_failed() {
        let mut entries = entries(&[1, 2]);
        entries[1].reason = PlaylistEntryReason::Failed(String::from("missing file"));

        assert_eq!(next_playable_after(&entries, 1), None);
    }

    fn entries(keys: &[u32]) -> Vec<PlaylistRuntimeEntry> {
        keys.iter()
            .map(|&key| PlaylistRuntimeEntry::dormant(key))
            .collect()
    }
}
