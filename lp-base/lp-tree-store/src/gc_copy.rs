//! Collecting one victim (invariant I2): copy each live record to the cold
//! head (the append reads every copy back and compares), and only then kill
//! and erase. Runs only right after a mark has pruned the index, so the
//! index entries in the victim are exactly its live records.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::heap_sort::heap_sort_by;
use crate::object_id::ObjectId;
use crate::record_log::RecordLog;
use crate::sector_header::HeadKind;
use crate::store_error::StoreError;

pub fn collect_sector<F: Flash>(
    log: &mut RecordLog<F>,
    victim: u32,
) -> Result<(), StoreError<F::Error>> {
    let mut items: Vec<(u32, ObjectId)> = log
        .index
        .positions_in(victim)
        .map(|i| (log.index.loc_at(i).offset, log.index.id_at(i)))
        .collect();
    heap_sort_by(&mut items, |a, b| a.0 < b.0);
    log.note(items.capacity() * core::mem::size_of::<(u32, ObjectId)>());
    #[cfg(feature = "mutants")]
    if mutant!(GcEraseBeforeCopy) {
        return crate::mutants::collect_erasing_first(log, victim, items);
    }
    log.gc_victim = Some(victim);
    for (_, id) in items {
        let (h, payload) = log.read_record(id)?;
        log.append(HeadKind::Cold, h.kind, h.codec, id, &[&payload])?;
        stat!(
            log.counters.gc_copies += 1;
            log.counters.gc_copy_bytes += u64::from(h.total_len());
        );
    }
    log.gc_victim = None;
    log.kill_and_erase(victim)?;
    Ok(())
}
