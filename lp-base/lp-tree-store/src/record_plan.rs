//! A commit's records, laid out in RAM before anything is written.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::node_layout::LaidRecord;
use crate::object_id::ObjectId;
use crate::record_header::RECORD_HEADER_LEN;
use crate::sector_header::HeadKind;

/// A record planned for a head.
#[derive(Clone, Debug)]
pub struct PlannedRecord {
    pub rec: LaidRecord,
    pub head: HeadKind,
}

impl PlannedRecord {
    pub fn total_len(&self) -> u32 {
        RECORD_HEADER_LEN + self.rec.payload.len() as u32
    }
}

/// Planned records in write order (the root last), by id.
#[derive(Clone, Debug, Default)]
pub struct RecordPlan {
    pub records: Vec<PlannedRecord>,
    by_id: BTreeMap<ObjectId, usize>,
}

impl RecordPlan {
    pub fn contains(&self, id: ObjectId) -> bool {
        self.by_id.contains_key(&id)
    }

    pub fn get(&self, id: ObjectId) -> Option<&LaidRecord> {
        self.by_id.get(&id).map(|&i| &self.records[i].rec)
    }

    /// Add unless already planned.
    pub fn push(&mut self, rec: LaidRecord, head: HeadKind) {
        if self.by_id.contains_key(&rec.id) {
            return;
        }
        self.by_id.insert(rec.id, self.records.len());
        self.records.push(PlannedRecord { rec, head });
    }

    pub fn total_bytes(&self) -> u64 {
        self.records.iter().map(|r| u64::from(r.total_len())).sum()
    }
}
