//! When a derived wire payload last changed, compared by content.
//!
//! Two read payloads are gated behind a revision that has no single source:
//!
//! - the binding-graph probe's **structure** (bindings, channel identities
//!   and their static fields) is DERIVED, per read, from the binding index,
//!   the scope tree, the panel-writer store and the authored defs' panel
//!   hints, and no one of those carries a revision that covers the rest;
//! - a node's **state slot root** (`node.<id>.state`) is live runtime data
//!   whose fields a node restamps on every produce even when the value holds
//!   (a product handle's revision is its content signal to the resolver), so
//!   the stamps say "touched", not "changed" (see `state_root_stamps`).
//!
//! Stamping each mutation site would miss the next one somebody adds, so the
//! engine compares by content, the way `control_geometry_stamps` does for
//! sample layouts: each read hashes what it built and keeps only that hash,
//! not the payload — a device would otherwise hold a second copy for nothing.
//! Nothing a mutation forgets to declare can slip past it.
//!
//! The stamp is the engine revision at the read that saw the change, and
//! always later than the stamp it replaces: a panel write or binding edit
//! can land between two reads without the engine revision moving, and two
//! different payloads must never answer to one revision.

use lpc_model::Revision;

/// One payload's change stamp: the hash of the last content a read
/// answered, and the revision at which it last changed.
#[derive(Debug, Default)]
pub(crate) struct ContentStamp {
    last: Option<(u64, Revision)>,
}

impl ContentStamp {
    /// The payload's revision, given the hash of the content a read just
    /// built at engine revision `now`.
    pub(crate) fn stamp(&mut self, content_hash: u64, now: Revision) -> Revision {
        let changed_at = match self.last {
            Some((hash, changed_at)) if hash == content_hash => return changed_at,
            Some((_, previous)) => now.max(previous.next()),
            None => now,
        };
        self.last = Some((content_hash, changed_at));
        changed_at
    }

    /// The last revision [`Self::stamp`] handed out, if it ever ran.
    pub(crate) fn changed_at(&self) -> Option<Revision> {
        self.last.map(|(_, changed_at)| changed_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_content_keeps_its_first_stamp() {
        let mut stamp = ContentStamp::default();
        assert_eq!(stamp.stamp(11, Revision::new(5)), Revision::new(5));
        assert_eq!(
            stamp.stamp(11, Revision::new(9)),
            Revision::new(5),
            "the same content at a later revision is not a change"
        );
    }

    #[test]
    fn changed_content_restamps_at_the_revision_that_saw_it() {
        let mut stamp = ContentStamp::default();
        stamp.stamp(11, Revision::new(5));
        assert_eq!(stamp.stamp(12, Revision::new(8)), Revision::new(8));
    }

    /// A panel write between two reads in one engine revision still changes
    /// the content: it must not answer to the revision the old one had.
    #[test]
    fn a_change_within_one_engine_revision_still_moves_the_stamp() {
        let mut stamp = ContentStamp::default();
        assert_eq!(stamp.stamp(11, Revision::new(5)), Revision::new(5));
        assert_eq!(stamp.stamp(12, Revision::new(5)), Revision::new(6));
        assert_eq!(
            stamp.stamp(11, Revision::new(5)),
            Revision::new(7),
            "going back to older content is a change too"
        );
    }
}
