//! When each node's state slot root last changed, by content.
//!
//! A project read sends a node's `node.<id>.state` root when it changed
//! after the client's `since`. The runtime's own stamps cannot say that: a
//! node restamps its produced fields on every produce (a product handle's
//! revision is the resolver's "the content behind this moved" signal, so it
//! has to), which made every alive node's state root ride every lens read —
//! a shader's root is nothing but its constant output handle, 231 B of it on
//! the PLAYFUL choker, read after read (lean-wire P6).
//!
//! So a read hashes each root it might send without its stamps
//! ([`super::state_root_values_hash`]) and keeps one [`ContentStamp`] per
//! node: the revision at which a read last saw that hash change. The root
//! rides when that stamp is newer than `since`.
//!
//! **A change is stamped one revision ahead** (`now + 1`). The hash is taken
//! before the read's probes run, and a probe renders — a render can move a
//! node's state at the read's own revision, after that read answered. A
//! client whose `since` is that revision would never hear about it if the
//! change were stamped `now`. One revision ahead, it rides the next read,
//! and a client that already had it gets it once more — the price of not
//! keeping per-client state. A root seen for the first time is stamped
//! `now`, which is what the entry-stamp gate this replaces promised.
//!
//! A client's mirror keeps the stamps of the last root it was sent; nothing
//! a client reads keys on a state field's stamp (previews carry their own
//! revisions), and a value change always restamps the root.

use alloc::vec::Vec;

use lpc_model::{NodeId, Revision};

use super::content_stamp::ContentStamp;

/// Per-node state-root change stamps.
#[derive(Debug, Default)]
pub(crate) struct StateRootStamps {
    entries: Vec<(NodeId, ContentStamp)>,
}

impl StateRootStamps {
    /// The revision at which `node`'s state root last changed, given the
    /// hash of the root a read sees at engine revision `now`: `now` for a
    /// root never seen before, `now + 1` (or later) for a changed one — see
    /// the module docs.
    pub(crate) fn stamp(&mut self, node: NodeId, root_hash: u64, now: Revision) -> Revision {
        match self.entries.iter_mut().find(|(id, _)| *id == node) {
            Some((_, stamp)) => stamp.stamp(root_hash, now.next()),
            None => {
                let mut stamp = ContentStamp::default();
                let changed_at = stamp.stamp(root_hash, now);
                self.entries.push((node, stamp));
                changed_at
            }
        }
    }

    /// The last stamp handed out for `node`, if a read has seen its root.
    pub(crate) fn changed_at(&self, node: NodeId) -> Option<Revision> {
        self.entries
            .iter()
            .find(|(id, _)| *id == node)
            .and_then(|(_, stamp)| stamp.changed_at())
    }

    /// Forget nodes that no longer hold a state root.
    pub(crate) fn retain(&mut self, keep: impl Fn(NodeId) -> bool) {
        self.entries.retain(|(id, _)| keep(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_root_keeps_its_stamp() {
        let mut stamps = StateRootStamps::default();
        let node = NodeId::new(4);
        assert_eq!(stamps.stamp(node, 7, Revision::new(12)), Revision::new(12));
        assert_eq!(stamps.stamp(node, 7, Revision::new(30)), Revision::new(12));
        assert_eq!(stamps.changed_at(node), Some(Revision::new(12)));
    }

    #[test]
    fn a_changed_root_restamps_one_revision_ahead() {
        let mut stamps = StateRootStamps::default();
        let node = NodeId::new(4);
        stamps.stamp(node, 7, Revision::new(12));
        assert_eq!(stamps.stamp(node, 8, Revision::new(30)), Revision::new(31));
    }

    /// A probe moved the state after the read at revision 30 answered; the
    /// next read, still at 30, must reach a client whose `since` is 30.
    #[test]
    fn a_change_seen_at_the_revision_a_client_already_read_still_reaches_it() {
        let mut stamps = StateRootStamps::default();
        let node = NodeId::new(4);
        stamps.stamp(node, 7, Revision::new(30));
        let changed_at = stamps.stamp(node, 8, Revision::new(30));
        assert!(changed_at > Revision::new(30), "stamped {changed_at:?}");
    }

    #[test]
    fn nodes_stamp_independently_and_forget_on_retain() {
        let mut stamps = StateRootStamps::default();
        stamps.stamp(NodeId::new(1), 7, Revision::new(5));
        stamps.stamp(NodeId::new(2), 7, Revision::new(9));
        assert_eq!(stamps.changed_at(NodeId::new(1)), Some(Revision::new(5)));
        stamps.retain(|id| id != NodeId::new(1));
        assert_eq!(stamps.changed_at(NodeId::new(1)), None);
        assert_eq!(stamps.changed_at(NodeId::new(2)), Some(Revision::new(9)));
    }
}
