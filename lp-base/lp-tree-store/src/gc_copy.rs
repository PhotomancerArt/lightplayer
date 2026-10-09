//! Collecting one victim (invariant I2): copy each live record to the cold
//! head, read the copy back and compare, and only then kill and erase.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::flash::Flash;
use crate::object_id::ObjectId;
use crate::ram_index::RecordLoc;
use crate::record_log::RecordLog;
use crate::sector_header::HeadKind;
use crate::small_sort::sort_small_by;
use crate::store_error::StoreError;

/// Records and bytes copied.
#[derive(Clone, Copy, Debug, Default)]
pub struct CopyCount {
    pub records: u64,
    pub bytes: u64,
}

pub fn collect_sector<F: Flash>(
    log: &mut RecordLog<F>,
    victim: u32,
    live: &BTreeSet<ObjectId>,
) -> Result<CopyCount, StoreError<F::Error>> {
    let mut items: Vec<(ObjectId, RecordLoc)> = log
        .index
        .iter()
        .filter(|(id, loc)| u32::from(loc.sector) == victim && live.contains(id))
        .collect();
    sort_small_by(&mut items, |a, b| a.1.offset < b.1.offset);
    log.gc_victim = Some(victim);
    let mut count = CopyCount::default();
    for (id, loc) in items {
        let raw = log.read_raw(loc)?;
        let new_loc = log.append(HeadKind::Cold, &raw, id, loc.kind, loc.codec)?;
        if log.read_raw(new_loc)? != raw {
            return Err(StoreError::Corrupt("GC copy does not read back"));
        }
        let v = &mut log.sectors.live_bytes[victim as usize];
        *v = v.saturating_sub(raw.len() as u32);
        count.records += 1;
        count.bytes += raw.len() as u64;
    }
    log.gc_victim = None;
    log.kill_and_erase(victim)?;
    Ok(count)
}
