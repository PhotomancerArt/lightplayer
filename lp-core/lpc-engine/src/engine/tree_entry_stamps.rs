//! When each tree entry's `entry_changed` content last changed.
//!
//! A project read sends a `WireTreeDelta::EntryChanged` for every entry whose
//! `change_frame` is newer than the client's `since`. The entry's own stamps
//! cannot answer that: every engine call takes a node's runtime out of its
//! entry and puts it back with `set_state(Alive(..), frame)`, so the stamp of
//! every alive node moved every frame and each rode every lens read with the
//! same status and state it had last time — 368 B of the PLAYFUL choker's
//! steady read (lean-wire P6's keep-list).
//!
//! So a read hashes what an `entry_changed` delta would carry — the entry's
//! status and its wire state (`Executing` reads as `Alive`, as it does on the
//! wire) — and keeps one [`ContentStamp`] per entry: the revision at which a
//! read last saw that content change. The delta's `change_frame` is that
//! stamp. Comparing content, not mutation sites, is the rule
//! [`super::content_stamp`] sets: no put-back, compile or status write can
//! slip past it or restamp without a change.
//!
//! **The fence.** The hash is taken when a read starts sending its tree, and
//! a read's probes run after that: a render probe compiles a shader and
//! stamps its `Error` status while the read serving revision `R` is already
//! past its tree deltas. The client's next `since` is `R`, so the change must
//! be stamped past `R` or it never arrives. A change the next refresh finds
//! is therefore stamped `now` only when no read has served `now` yet (a tick
//! moved the revision since, and every client's `since` is older), and
//! `served + 1` otherwise. That is `state_root_stamps`' "one revision ahead",
//! paid only when a read already served the revision, so a change made
//! during a tick is not delivered twice.

use alloc::vec::Vec;

use lpc_model::{NodeId, Revision};
use lpc_wire::{NodeRuntimeStatus, WireEntryState};

use super::content_stamp::ContentStamp;
use crate::node::RuntimeNodeTree;

/// Per-entry content stamps for the tree deltas' `change_frame`.
#[derive(Debug, Default)]
pub(crate) struct TreeEntryStamps {
    entries: Vec<(NodeId, ContentStamp)>,
    /// The newest revision a refresh ran at: a read has served tree deltas
    /// at it, so a change found at it again landed after they were sent.
    served: Option<Revision>,
}

impl TreeEntryStamps {
    /// Hash every entry of `tree` at engine revision `now`, restamp those
    /// whose content moved, and forget entries the tree no longer holds. A
    /// read runs this right before it streams tree deltas.
    pub(crate) fn refresh<N>(&mut self, tree: &RuntimeNodeTree<N>, now: Revision) {
        let changed_at = match self.served {
            Some(served) if served >= now => served.next(),
            _ => now,
        };
        for entry in tree.entries() {
            let wire_state = WireEntryState::from(entry.state.value());
            let hash = entry_content_hash(entry.status.value(), &wire_state);
            match self.entries.iter_mut().find(|(id, _)| *id == entry.id) {
                Some((_, stamp)) => {
                    stamp.stamp(hash, changed_at);
                }
                None => {
                    let mut stamp = ContentStamp::default();
                    stamp.stamp(hash, changed_at);
                    self.entries.push((entry.id, stamp));
                }
            }
        }
        self.entries.retain(|(id, _)| tree.get(*id).is_some());
        self.served = Some(self.served.map_or(now, |served| served.max(now)));
    }

    /// The revision at which `node`'s `entry_changed` content last changed,
    /// as of the last [`Self::refresh`]; `None` for an entry it never saw.
    pub(crate) fn changed_at(&self, node: NodeId) -> Option<Revision> {
        self.entries
            .iter()
            .find(|(id, _)| *id == node)
            .and_then(|(_, stamp)| stamp.changed_at())
    }
}

/// A hash of an `entry_changed` delta's content: the FNV-1a 64 of each
/// half's wire bytes (the same bytes a client would see), folded. Two
/// hashes of the halves, not one of a tuple, because the firmware already
/// carries each half's serializer and a tuple's would be new code (−144 B).
fn entry_content_hash(status: &NodeRuntimeStatus, state: &WireEntryState) -> u64 {
    lpc_wire::ser_write_json_fnv64(status) ^ lpc_wire::ser_write_json_fnv64(state).rotate_left(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{NodeEntryState, test_placeholder_spine};
    use lpc_model::{NodeName, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    #[test]
    fn a_put_back_with_the_same_content_keeps_the_stamp() {
        let (mut tree, node) = tree_with_child();
        let mut stamps = TreeEntryStamps::default();
        stamps.refresh(&tree, Revision::new(10));
        let first = stamps.changed_at(node).expect("stamped");

        // What every engine call does to an alive node: take it out, put it
        // back, at a newer frame.
        let entry = tree.get_mut(node).unwrap();
        entry.set_state(NodeEntryState::Alive(()), Revision::new(11));
        entry.set_status(NodeRuntimeStatus::Ok, Revision::new(11));
        entry.set_status(NodeRuntimeStatus::Ok, Revision::new(12));

        stamps.refresh(&tree, Revision::new(12));
        assert_eq!(stamps.changed_at(node), Some(first));
    }

    #[test]
    fn a_status_change_made_during_a_tick_is_stamped_at_that_revision() {
        let (mut tree, node) = tree_with_child();
        let mut stamps = TreeEntryStamps::default();
        stamps.refresh(&tree, Revision::new(10));

        tree.get_mut(node)
            .unwrap()
            .set_status(NodeRuntimeStatus::Error("boom".into()), Revision::new(11));
        stamps.refresh(&tree, Revision::new(11));
        assert_eq!(stamps.changed_at(node), Some(Revision::new(11)));
    }

    /// A render probe changed the status after the read at revision 20 sent
    /// its tree; the next read, still at 20, must reach a client whose
    /// `since` is 20 — though the entry last changed long before.
    #[test]
    fn a_change_found_at_a_served_revision_is_stamped_past_it() {
        let (mut tree, node) = tree_with_child();
        let mut stamps = TreeEntryStamps::default();
        stamps.refresh(&tree, Revision::new(5));
        stamps.refresh(&tree, Revision::new(20));
        assert_eq!(stamps.changed_at(node), Some(Revision::new(5)));

        tree.get_mut(node)
            .unwrap()
            .set_status(NodeRuntimeStatus::Error("boom".into()), Revision::new(20));
        stamps.refresh(&tree, Revision::new(20));
        assert_eq!(stamps.changed_at(node), Some(Revision::new(21)));
    }

    #[test]
    fn a_failed_reason_is_content() {
        let (mut tree, node) = tree_with_child();
        let mut stamps = TreeEntryStamps::default();
        let entry = tree.get_mut(node).unwrap();
        entry.set_state(
            NodeEntryState::Failed {
                reason: "first".into(),
            },
            Revision::new(3),
        );
        stamps.refresh(&tree, Revision::new(3));
        tree.get_mut(node).unwrap().set_state(
            NodeEntryState::Failed {
                reason: "second".into(),
            },
            Revision::new(4),
        );
        stamps.refresh(&tree, Revision::new(4));
        assert_eq!(stamps.changed_at(node), Some(Revision::new(4)));
    }

    #[test]
    fn removed_entries_are_forgotten() {
        let (mut tree, node) = tree_with_child();
        let mut stamps = TreeEntryStamps::default();
        stamps.refresh(&tree, Revision::new(3));
        tree.remove_subtree(node, Revision::new(4)).unwrap();
        stamps.refresh(&tree, Revision::new(4));
        assert_eq!(stamps.changed_at(node), None);
        assert!(stamps.changed_at(tree.root()).is_some());
    }

    fn tree_with_child() -> (RuntimeNodeTree<()>, NodeId) {
        let mut tree =
            RuntimeNodeTree::new(TreePath::parse("/root.show").unwrap(), Revision::new(0));
        let root = tree.root();
        let node = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                test_placeholder_spine(),
                Revision::new(1),
            )
            .unwrap();
        let entry = tree.get_mut(node).unwrap();
        entry.set_state(NodeEntryState::Alive(()), Revision::new(2));
        entry.set_status(NodeRuntimeStatus::Ok, Revision::new(2));
        (tree, node)
    }
}
