//! Which playlist entries are loaded: the registry's residency set.
//!
//! A playlist entry is either **resident** (its whole subtree is derived into
//! the effective inventory, so the engine can build it) or **dormant** (it is
//! absent everywhere: no tree node, no def or asset rows, no artifact-store
//! locations). Only the playlist's own def remembers a dormant entry — its
//! key, name, ref and duration.
//!
//! The registry owns this set because it re-derives the *whole* inventory on
//! every mutation. If dormancy lived only in the engine, the next edit would
//! see every dormant entry as added and load it again. Derivation therefore
//! consults [`EntryResidency`] at each playlist entry's `Ref` invocation and
//! stops there when the entry is dormant.
//!
//! Who decides: the playlist asks (a polled request on its runtime node), and
//! the tick owner applies the request through
//! [`crate::ProjectRegistry::set_entry_resident`] /
//! [`crate::ProjectRegistry::make_only_resident`] before the engine ticks.
//! The registry only records the answer and re-derives.
//!
//! The default, for a playlist with no explicit set, is its authored
//! `idle_entry` only. The default is read from the effective playlist def at
//! derivation time, so nothing is stored for a playlist that was never
//! switched. Sets are keyed by the playlist's [`NodeUseLocation`] and dropped
//! when that use leaves the tree.

use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::NodeUseLocation;

/// Per-playlist resident entry keys; absent playlists use the default
/// (`idle_entry` only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntryResidency {
    /// Explicit resident sets, sorted and deduplicated, by playlist use.
    explicit: VecMap<NodeUseLocation, Vec<u32>>,
}

impl EntryResidency {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `entry` of the playlist at `playlist` is resident.
    ///
    /// `idle_entry` is the playlist's effective authored idle entry; it is the
    /// whole resident set when no explicit set was recorded.
    pub fn is_resident(&self, playlist: &NodeUseLocation, entry: u32, idle_entry: u32) -> bool {
        match self.explicit.get(playlist) {
            Some(set) => set.binary_search(&entry).is_ok(),
            None => entry == idle_entry,
        }
    }

    /// The explicit resident set of `playlist`, or `None` when the default
    /// (`idle_entry` only) applies.
    pub fn explicit_set(&self, playlist: &NodeUseLocation) -> Option<&[u32]> {
        self.explicit.get(playlist).map(Vec::as_slice)
    }

    /// Record `entry` as resident or dormant. Returns whether the effective
    /// set changed.
    ///
    /// The first change to a playlist materializes its default (`idle_entry`
    /// only) as an explicit set before applying the change.
    pub(crate) fn set_resident(
        &mut self,
        playlist: &NodeUseLocation,
        entry: u32,
        resident: bool,
        idle_entry: u32,
    ) -> bool {
        if self.is_resident(playlist, entry, idle_entry) == resident {
            return false;
        }
        if !self.explicit.contains_key(playlist) {
            self.explicit
                .insert(playlist.clone(), alloc::vec![idle_entry]);
        }
        let set = self
            .explicit
            .get_mut(playlist)
            .expect("explicit set inserted above");
        match set.binary_search(&entry) {
            Ok(index) if !resident => {
                set.remove(index);
            }
            Err(index) if resident => set.insert(index, entry),
            _ => {}
        }
        true
    }

    /// Make `entry` the only resident entry of `playlist`. Returns whether
    /// the effective set changed.
    pub(crate) fn make_only_resident(
        &mut self,
        playlist: &NodeUseLocation,
        entry: u32,
        idle_entry: u32,
    ) -> bool {
        let unchanged = match self.explicit.get(playlist) {
            Some(set) => set.as_slice() == [entry],
            None => entry == idle_entry,
        };
        if unchanged {
            return false;
        }
        self.explicit.insert(playlist.clone(), alloc::vec![entry]);
        true
    }

    /// Drop explicit sets for playlists `keep` rejects (uses that left the
    /// tree), so a playlist that comes back starts from its default again.
    pub(crate) fn retain_playlists(&mut self, mut keep: impl FnMut(&NodeUseLocation) -> bool) {
        self.explicit.retain(|playlist, _| keep(playlist));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::SlotPath;

    #[test]
    fn default_is_idle_entry_only() {
        let residency = EntryResidency::new();
        let playlist = playlist();
        assert!(residency.is_resident(&playlist, 1, 1));
        assert!(!residency.is_resident(&playlist, 2, 1));
        assert!(residency.explicit_set(&playlist).is_none());
    }

    #[test]
    fn first_change_materializes_the_default_then_applies() {
        let mut residency = EntryResidency::new();
        let playlist = playlist();
        assert!(residency.set_resident(&playlist, 3, true, 1));
        assert_eq!(residency.explicit_set(&playlist), Some(&[1, 3][..]));
        assert!(residency.set_resident(&playlist, 1, false, 1));
        assert_eq!(residency.explicit_set(&playlist), Some(&[3][..]));
        assert!(!residency.is_resident(&playlist, 1, 1));
    }

    #[test]
    fn no_op_changes_report_unchanged_and_store_nothing() {
        let mut residency = EntryResidency::new();
        let playlist = playlist();
        assert!(!residency.set_resident(&playlist, 1, true, 1));
        assert!(!residency.set_resident(&playlist, 2, false, 1));
        assert!(!residency.make_only_resident(&playlist, 1, 1));
        assert!(residency.explicit_set(&playlist).is_none());
    }

    #[test]
    fn make_only_resident_replaces_the_set() {
        let mut residency = EntryResidency::new();
        let playlist = playlist();
        residency.set_resident(&playlist, 2, true, 1);
        assert!(residency.make_only_resident(&playlist, 3, 1));
        assert_eq!(residency.explicit_set(&playlist), Some(&[3][..]));
        assert!(!residency.make_only_resident(&playlist, 3, 1));
    }

    #[test]
    fn retain_playlists_drops_sets_back_to_the_default() {
        let mut residency = EntryResidency::new();
        let playlist = playlist();
        residency.make_only_resident(&playlist, 2, 1);
        residency.retain_playlists(|_| false);
        assert!(residency.explicit_set(&playlist).is_none());
        assert!(residency.is_resident(&playlist, 1, 1));
    }

    fn playlist() -> NodeUseLocation {
        NodeUseLocation::root().child(SlotPath::parse("nodes[playlist]").unwrap())
    }
}
