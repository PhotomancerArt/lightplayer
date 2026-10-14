//! Collecting one victim (invariant I2): copy each live record to the head
//! of the victim's own kind (the append reads every copy back and
//! compares), and only then kill and erase. Runs only right after a mark
//! has pruned the index, so the index entries in the victim are exactly its
//! live records, and the mark's lengths (`lens`, one per index entry) say
//! how long each is.
//!
//! Packing (`pack`, every victim but a renewed head): each copy is the
//! largest record left that fits what the head has left, and before one
//! that fits nowhere opens a sector, the head's tail is topped off with the
//! largest live records of its kind, from sectors this pass has not written,
//! that fit it (their old copies become garbage there, which a later
//! victim collects). Copied in sector order, a victim whose first record
//! did not fit the head's tail opened a head at once and left that tail
//! unused; filling it from the victim alone still left every sector closed
//! with whatever tail its victim's own records could not fill — near full,
//! three 1,024 B chunks and a 1,000 B tail no chunk fits, while the small
//! records that could fill those tails sat in other sectors — and GC
//! cycled one sector short of a packing the bound had counted on (the
//! 2026-10-10 defects).

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::heap_sort::heap_sort_by;
use crate::record_log::RecordLog;
use crate::sector_header::HeadKind;
use crate::store_error::StoreError;

/// Collect `victim`. `pass_from`: the first sector sequence this GC pass
/// opened (sectors from it on hold its own copies, already packed).
pub fn collect_sector<F: Flash>(
    log: &mut RecordLog<F>,
    victim: u32,
    pack: bool,
    lens: &[u16],
    pass_from: u32,
) -> Result<(), StoreError<F::Error>> {
    // (offset, index position), in sector order.
    let mut items: Vec<(u32, usize)> = log
        .index
        .positions_in(victim)
        .map(|i| (log.index.loc_at(i).offset, i))
        .collect();
    heap_sort_by(&mut items, |a, b| a.0 < b.0);
    log.note(items.capacity() * core::mem::size_of::<(u32, usize)>());
    #[cfg(feature = "mutants")]
    if mutant!(GcEraseBeforeCopy) {
        let items = items
            .iter()
            .map(|&(o, i)| (o, log.index.id_at(i)))
            .collect();
        return crate::mutants::collect_erasing_first(log, victim, items);
    }
    let head = log.sectors.kind(victim);
    // A head being collected (renewed) stops being one: its copies open a
    // new head of its kind.
    for h in &mut log.heads {
        if *h == Some(victim) {
            *h = None;
        }
    }
    log.gc_victim = Some(victim);
    let len_of = |i: usize| u32::from(lens[i]);
    while !items.is_empty() {
        let mut next = 0;
        if pack {
            let room = log.head_remaining(head);
            let mut best = 0;
            for (k, &(_, i)) in items.iter().enumerate() {
                if len_of(i) <= room && len_of(i) > best {
                    (best, next) = (len_of(i), k);
                }
            }
        }
        let (_, i) = items.remove(next);
        if pack && len_of(i) > log.head_remaining(head) {
            top_off(log, head, victim, lens, pass_from)?;
        }
        copy(log, head, i)?;
    }
    log.gc_victim = None;
    log.kill_and_erase(victim)?;
    Ok(())
}

/// Whether sector `s` was opened at or after sequence `from` (wrapping, as
/// the sequence does).
pub fn opened_since<F: Flash>(log: &RecordLog<F>, s: u32, from: u32) -> bool {
    log.sectors.seq[s as usize].wrapping_sub(from) as i32 >= 0
}

/// Fill the `kind` head's tail with the largest live records of its kind
/// that fit it, from sectors other than `victim`, the heads and this pass's
/// own; each one's sector loses its bytes from its (still exact) live count.
fn top_off<F: Flash>(
    log: &mut RecordLog<F>,
    kind: HeadKind,
    victim: u32,
    lens: &[u16],
    pass_from: u32,
) -> Result<(), StoreError<F::Error>> {
    while log.heads[kind.index()].is_some() {
        let room = log.head_remaining(kind);
        let mut best: Option<(u32, usize)> = None;
        for (i, &len) in lens.iter().enumerate() {
            let len = u32::from(len);
            if len > room || best.is_some_and(|b| b.0 >= len) {
                continue;
            }
            let s = log.index.loc_at(i).sector;
            if s != victim
                && !log.is_head(s)
                && log.sectors.kind(s) == kind
                && !opened_since(log, s, pass_from)
            {
                best = Some((len, i));
            }
        }
        let Some((len, i)) = best else {
            return Ok(());
        };
        let from = log.index.loc_at(i).sector as usize;
        copy(log, kind, i)?;
        let l = &mut log.sectors.live[from];
        *l = l.saturating_sub(len as u16);
    }
    Ok(())
}

/// Copy the record at index position `i` to the `kind` head (the head a
/// record goes to is the kind of the sector it is in: a hot sector's live
/// records are the committed root, the hot directory and panel files, which
/// the next commits replace, so they stay with the hot data; and when a cut
/// closed the hot head, collecting it gives a hot head back at least the
/// room the cut took, for no free sector — copied to the cold head, they
/// cost the sector collecting it freed whenever the cold head was full, the
/// 2026-10-09 defect's second mechanism).
fn copy<F: Flash>(
    log: &mut RecordLog<F>,
    kind: HeadKind,
    i: usize,
) -> Result<(), StoreError<F::Error>> {
    let id = log.index.id_at(i);
    let (h, payload) = log.read_record(id)?;
    log.append(kind, h.kind, h.codec, id, &[&payload])?;
    stat!(
        log.counters.gc_copies += 1;
        log.counters.gc_copy_bytes += u64::from(h.total_len());
    );
    Ok(())
}
