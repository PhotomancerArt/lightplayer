//! The RAM index: id → where its one trusted copy lives. Rebuilt at mount by
//! scanning; nothing on flash describes it.

use alloc::collections::BTreeMap;
use core::mem::size_of;

use crate::object_id::ObjectId;
use crate::record_header::RECORD_HEADER_LEN;
use crate::record_kind::{ChunkCodec, RecordKind};

/// Where a record is, and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLoc {
    pub sector: u16,
    /// Offset of the record header within the sector.
    pub offset: u16,
    /// Payload length.
    pub len: u16,
    pub kind: RecordKind,
    pub codec: ChunkCodec,
}

impl RecordLoc {
    /// Header + payload.
    pub fn total_len(&self) -> u32 {
        RECORD_HEADER_LEN + u32::from(self.len)
    }
}

/// id → location, one location per id (duplicates on flash are harmless:
/// the index names the newest-sector copy found at mount, or the copy this
/// session wrote last).
#[derive(Clone, Debug, Default)]
pub struct RamIndex {
    map: BTreeMap<ObjectId, RecordLoc>,
}

impl RamIndex {
    pub fn get(&self, id: ObjectId) -> Option<RecordLoc> {
        self.map.get(&id).copied()
    }

    pub fn contains(&self, id: ObjectId) -> bool {
        self.map.contains_key(&id)
    }

    pub fn insert(&mut self, id: ObjectId, loc: RecordLoc) {
        self.map.insert(id, loc);
    }

    /// Forget every record in `sector` (it is about to be erased).
    pub fn remove_sector(&mut self, sector: u32) {
        self.map.retain(|_, loc| u32::from(loc.sector) != sector);
    }

    pub fn iter(&self) -> impl Iterator<Item = (ObjectId, RecordLoc)> + '_ {
        self.map.iter().map(|(k, v)| (*k, *v))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// The honest floor: entries × the entry's size (the B-tree's own node
    /// overhead is not counted; a sorted array on device would be this).
    pub fn ram_bytes(&self) -> usize {
        self.map.len() * (size_of::<ObjectId>() + size_of::<RecordLoc>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_sector() {
        let mut ix = RamIndex::default();
        let loc = |sector| RecordLoc {
            sector,
            offset: 20,
            len: 4,
            kind: RecordKind::Blob,
            codec: ChunkCodec::Stored,
        };
        ix.insert(ObjectId(1), loc(0));
        ix.insert(ObjectId(2), loc(1));
        ix.remove_sector(0);
        assert!(!ix.contains(ObjectId(1)));
        assert_eq!(ix.get(ObjectId(2)), Some(loc(1)));
        assert_eq!(ix.ram_bytes(), 16);
    }
}
