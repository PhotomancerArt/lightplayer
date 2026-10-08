//! Invariant I1: the committed state is the valid root with the highest
//! sequence whose closure is complete; if the newest is incomplete, the next.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_mark::mark;
use crate::object_id::ObjectId;
use crate::record_log::RecordLog;
use crate::record_plan::RecordPlan;
use crate::root_record::RootRecord;
use crate::small_sort::sort_small_by;
use crate::store_error::StoreError;

/// The chosen root, its id and its live set.
pub struct SelectedRoot {
    pub id: ObjectId,
    pub root: RootRecord,
    pub live: BTreeSet<ObjectId>,
}

/// Pick by I1 among `(seq, id)` candidates. `None` = no complete root.
pub fn select_root<F: Flash>(
    log: &mut RecordLog<F>,
    mut roots: Vec<(u64, ObjectId)>,
) -> Result<Option<SelectedRoot>, StoreError<F::Error>> {
    sort_small_by(&mut roots, |a, b| a > b);
    roots.dedup();
    let empty = RecordPlan::default();
    for (_, id) in roots {
        match mark(log, &[id], &empty) {
            Ok(live) => {
                let (_, payload) = log.read_payload(id)?;
                let Some(root) = RootRecord::decode(&payload) else {
                    continue;
                };
                return Ok(Some(SelectedRoot { id, root, live }));
            }
            Err(StoreError::Corrupt(_)) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}
