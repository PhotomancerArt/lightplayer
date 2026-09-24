//! When the binding graph's structure last changed.
//!
//! The binding-graph probe gates its structure (bindings, channel identities
//! and their static fields) behind one revision, and the values ride every
//! read. That revision must move whenever the structure moves and never
//! otherwise — but the structure is DERIVED, per read, from the binding index,
//! the scope tree, the panel-writer store and the authored defs' panel hints,
//! and no one of those carries a revision that covers the rest. Stamping each
//! mutation site would miss the next one somebody adds.
//!
//! So the engine compares by content, the way `control_geometry_stamps` does
//! for sample layouts: each read hashes the structure it built (the wire
//! bytes, `lpc_wire::ser_write_json_fnv64`) and keeps only that hash, not the
//! structure — a device would otherwise hold a second copy of the graph for
//! nothing. Nothing a mutation forgets to declare can slip past it.
//!
//! The stamp is the engine revision at the read that saw the change, and
//! always later than the stamp it replaces: a panel write or binding edit
//! can land between two reads without the engine revision moving, and two
//! different structures must never answer to one revision.

use lpc_model::Revision;

/// The binding structure's change stamp: the hash of the last structure a
/// probe answered, and the revision at which it last changed.
#[derive(Debug, Default)]
pub(crate) struct BindingStructureStamp {
    last: Option<(u64, Revision)>,
}

impl BindingStructureStamp {
    /// The structure revision, given the hash of the structure a probe just
    /// built at engine revision `now`.
    pub(crate) fn stamp(&mut self, structure_hash: u64, now: Revision) -> Revision {
        let changed_at = match self.last {
            Some((hash, changed_at)) if hash == structure_hash => return changed_at,
            Some((_, previous)) => now.max(previous.next()),
            None => now,
        };
        self.last = Some((structure_hash, changed_at));
        changed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_structure_keeps_its_first_stamp() {
        let mut stamp = BindingStructureStamp::default();
        assert_eq!(stamp.stamp(11, Revision::new(5)), Revision::new(5));
        assert_eq!(
            stamp.stamp(11, Revision::new(9)),
            Revision::new(5),
            "the same structure at a later revision is not a change"
        );
    }

    #[test]
    fn a_changed_structure_restamps_at_the_revision_that_saw_it() {
        let mut stamp = BindingStructureStamp::default();
        stamp.stamp(11, Revision::new(5));
        assert_eq!(stamp.stamp(12, Revision::new(8)), Revision::new(8));
    }

    /// A panel write between two reads in one engine revision still changes
    /// the structure: it must not answer to the revision the old one had.
    #[test]
    fn a_change_within_one_engine_revision_still_moves_the_stamp() {
        let mut stamp = BindingStructureStamp::default();
        assert_eq!(stamp.stamp(11, Revision::new(5)), Revision::new(5));
        assert_eq!(stamp.stamp(12, Revision::new(5)), Revision::new(6));
        assert_eq!(
            stamp.stamp(11, Revision::new(5)),
            Revision::new(7),
            "going back to an older structure is a change too"
        );
    }
}
