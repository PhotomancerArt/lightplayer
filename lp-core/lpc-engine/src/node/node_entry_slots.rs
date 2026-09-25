//! The node tree's entry storage: live entries only, sorted by id.
//!
//! [`crate::node::RuntimeNodeTree`] used a dense `Vec<Option<_>>` indexed by
//! [`NodeId`]. Ids are never reused, so every removed node left a `None`
//! tombstone behind for good, and dormant playlist entries — which remove
//! and re-attach a subtree on every pattern switch — would have grown it
//! without bound over a tour
//! (`docs/defects/2026-09-25-node-tree-tombstones-grow-per-reload.md`).
//!
//! Here a removed entry is dropped with its slot, so storage tracks the live
//! count. Lookup stays O(1) in the common case: ids are handed out in
//! increasing order and never reused, so an entry's position is never above
//! its id, and it *equals* its id until something below it is removed. A
//! lookup tries that position first and binary-searches only the prefix
//! below it on a miss.

use alloc::vec::Vec;

use lpc_model::NodeId;

use super::RuntimeNodeEntry;

/// Live node entries, sorted by [`NodeId`], with no tombstones.
#[derive(Debug)]
pub(super) struct NodeEntrySlots<N> {
    /// Sorted by `entry.id`, strictly increasing.
    entries: Vec<RuntimeNodeEntry<N>>,
}

impl<N> NodeEntrySlots<N> {
    pub(super) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The entry for `id`, if it is live.
    pub(super) fn get(&self, id: NodeId) -> Option<&RuntimeNodeEntry<N>> {
        self.position(id).map(|index| &self.entries[index])
    }

    /// The entry for `id`, mutably, if it is live.
    pub(super) fn get_mut(&mut self, id: NodeId) -> Option<&mut RuntimeNodeEntry<N>> {
        self.position(id).map(|index| &mut self.entries[index])
    }

    /// Append `entry`, whose id must be greater than every live id — the
    /// tree's `next_id` is monotonic, so a new node always lands at the end.
    pub(super) fn push(&mut self, entry: RuntimeNodeEntry<N>) {
        debug_assert!(
            self.entries.last().is_none_or(|last| last.id < entry.id),
            "node ids are allocated in increasing order"
        );
        self.entries.push(entry);
    }

    /// Remove and return the entry for `id`, releasing its slot.
    pub(super) fn remove(&mut self, id: NodeId) -> Option<RuntimeNodeEntry<N>> {
        self.position(id).map(|index| self.entries.remove(index))
    }

    /// Live entries in id order.
    pub(super) fn iter(&self) -> core::slice::Iter<'_, RuntimeNodeEntry<N>> {
        self.entries.iter()
    }

    /// Live entries in id order, mutably.
    pub(super) fn iter_mut(&mut self) -> core::slice::IterMut<'_, RuntimeNodeEntry<N>> {
        self.entries.iter_mut()
    }

    /// Number of live entries.
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Slots held, live or spare. Bounded by the most entries ever live at
    /// once, never by how many ids were minted.
    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Where `id` sits. Positions never exceed ids (strictly increasing ids
    /// from 0), so the entry is at `id` itself unless something below it was
    /// removed, and otherwise somewhere before it.
    fn position(&self, id: NodeId) -> Option<usize> {
        let guess = id.0 as usize;
        match self.entries.get(guess) {
            Some(entry) if entry.id == id => Some(guess),
            _ => {
                let end = guess.min(self.entries.len());
                self.entries[..end]
                    .binary_search_by_key(&id, |entry| entry.id)
                    .ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{Revision, TreePath};

    #[test]
    fn lookup_hits_at_the_id_before_any_removal() {
        let slots = slots_with(&[0, 1, 2, 3]);
        for id in 0..4 {
            assert_eq!(slots.get(NodeId::new(id)).unwrap().id, NodeId::new(id));
        }
        assert!(slots.get(NodeId::new(4)).is_none());
    }

    #[test]
    fn lookup_finds_entries_shifted_below_their_id() {
        let mut slots = slots_with(&[0, 1, 2, 3, 4]);
        assert_eq!(slots.remove(NodeId::new(1)).unwrap().id, NodeId::new(1));
        assert_eq!(slots.remove(NodeId::new(3)).unwrap().id, NodeId::new(3));
        slots.push(entry(9));

        let ids: Vec<u32> = slots.iter().map(|e| e.id.0).collect();
        assert_eq!(ids, [0, 2, 4, 9]);
        for id in [0, 2, 4, 9] {
            assert_eq!(slots.get(NodeId::new(id)).unwrap().id, NodeId::new(id));
        }
        for id in [1, 3, 5, 8, 10, 1000] {
            assert!(slots.get(NodeId::new(id)).is_none(), "id {id}");
        }
        assert!(slots.remove(NodeId::new(1)).is_none());
    }

    #[test]
    fn capacity_follows_live_entries_not_minted_ids() {
        let mut slots = slots_with(&[0]);
        let mut next = 1;
        for _ in 0..100 {
            for _ in 0..3 {
                slots.push(entry(next));
                next += 1;
            }
            for id in next - 3..next {
                slots.remove(NodeId::new(id)).unwrap();
            }
        }
        assert_eq!(slots.len(), 1);
        assert!(slots.capacity() <= 4, "capacity {}", slots.capacity());
    }

    fn slots_with(ids: &[u32]) -> NodeEntrySlots<()> {
        let mut slots = NodeEntrySlots::new();
        for &id in ids {
            slots.push(entry(id));
        }
        slots
    }

    fn entry(id: u32) -> RuntimeNodeEntry<()> {
        RuntimeNodeEntry::new(
            NodeId::new(id),
            TreePath::parse("/root.show").unwrap(),
            None,
            None,
            Revision::new(0),
        )
    }
}
