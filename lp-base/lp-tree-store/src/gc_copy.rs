//! Collecting one victim (invariant I2): copy each live record to the head
//! of the victim's own kind (the append reads every copy back and
//! compares), and only then kill and erase. Runs only right after a mark
//! has pruned the index, so the index entries in the victim are exactly its
//! live records.

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
    let head = victim_kind(log, victim)?;
    // A head being collected (renewed) stops being one: its copies open a
    // new head of its kind.
    for h in &mut log.heads {
        if *h == Some(victim) {
            *h = None;
        }
    }
    log.gc_victim = Some(victim);
    for (_, id) in items {
        let (h, payload) = log.read_record(id)?;
        log.append(head, h.kind, h.codec, id, &[&payload])?;
        stat!(
            log.counters.gc_copies += 1;
            log.counters.gc_copy_bytes += u64::from(h.total_len());
        );
    }
    log.gc_victim = None;
    log.kill_and_erase(victim)?;
    Ok(())
}

/// The head a victim's records are copied to: the kind its header names.
/// A hot sector's live records are the committed root, the hot directory and
/// panel files, which the next commits replace: copied to the hot head they
/// stay with the hot data. And when a cut closed the hot head (a torn or
/// half-programmed record in its tail), collecting it gives a hot head back
/// at least the room the cut took, for no free sector — copied to the cold
/// head, they cost the sector collecting it freed whenever the cold head was
/// full (the 2026-10-09 defect's second mechanism).
fn victim_kind<F: Flash>(
    log: &mut RecordLog<F>,
    victim: u32,
) -> Result<HeadKind, StoreError<F::Error>> {
    // Header bytes 4..8: version, head kind, log2 of the sector size. The
    // header was checked whole when the sector was mounted or opened.
    let mut b = [0u8; 4];
    log.read(log.addr(victim, 4), &mut b)?;
    Ok(if b[2] == HeadKind::Hot.index() as u8 {
        HeadKind::Hot
    } else {
        HeadKind::Cold
    })
}
