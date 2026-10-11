//! A mounted store's three numbers, always on (the `stats` feature is off in
//! firmware): what a boot line says about the store.

use crate::flash::Flash;
use crate::object_hasher::ObjectHasher;
use crate::tree_store::TreeStore;

/// What a mounted store looks like from outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MountSummary {
    /// Sectors in the partition.
    pub sectors: u32,
    /// Sectors free right now (by the live upper bound).
    pub free_sectors: u32,
    /// The committed root's sequence number (1 after a format; 0 never
    /// happens on a mounted store).
    pub root_seq: u64,
}

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// The store's sector count, free sectors and committed root sequence.
    pub fn summary(&self) -> MountSummary {
        MountSummary {
            sectors: self.log.sector_count,
            free_sectors: self.log.free_count(),
            root_seq: self.committed.as_ref().map_or(0, |c| c.root.seq),
        }
    }
}
