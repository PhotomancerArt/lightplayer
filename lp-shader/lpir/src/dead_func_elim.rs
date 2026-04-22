//! Remove local functions with zero remaining call sites that aren't roots.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;

use crate::lpir_module::LpirModule;
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId};

/// Counters returned by [`dead_func_elim`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DeadFuncElimResult {
    pub functions_removed: usize,
}

/// Local caller → callees (local [`CalleeRef::Local`] only, deduplicated per caller).
fn build_local_adjacency(module: &LpirModule) -> BTreeMap<FuncId, BTreeSet<FuncId>> {
    let mut adj: BTreeMap<FuncId, BTreeSet<FuncId>> = BTreeMap::new();
    for (&caller_id, func) in &module.functions {
        for op in &func.body {
            if let LpirOp::Call {
                callee: CalleeRef::Local(callee_id),
                ..
            } = op
            {
                adj.entry(caller_id).or_default().insert(*callee_id);
            }
        }
    }
    adj
}

/// Remove functions that aren't transitively reachable from `roots`.
///
/// Stable [`FuncId`] (M0) means deletion never invalidates surviving call sites.
/// Re-entry / cycles among reachable functions are handled by transitive marking.
pub fn dead_func_elim(module: &mut LpirModule, roots: &[FuncId]) -> DeadFuncElimResult {
    let adj = build_local_adjacency(module);

    let mut reachable: BTreeSet<FuncId> = BTreeSet::new();
    let mut work: VecDeque<FuncId> = VecDeque::new();
    for &r in roots {
        if module.functions.contains_key(&r) {
            if reachable.insert(r) {
                work.push_back(r);
            }
        } else {
            log::warn!("dead_func_elim: root func={r:?} not in module, ignoring");
        }
    }

    while let Some(f) = work.pop_front() {
        if let Some(callees) = adj.get(&f) {
            for &c in callees {
                if reachable.insert(c) {
                    work.push_back(c);
                }
            }
        }
    }

    let mut to_remove: Vec<FuncId> = module
        .functions
        .keys()
        .filter(|id| !reachable.contains(*id))
        .copied()
        .collect();

    to_remove.sort();
    let removed = to_remove.len();

    for id in to_remove {
        if let Some(f) = module.functions.remove(&id) {
            log::debug!("dead_func_elim: drop func={id:?} name={:?}", f.name);
        }
    }

    let kept = module.functions.len();
    let roots_n = roots.len();
    log::info!("dead_func_elim: removed={removed} kept={kept} roots={roots_n}");
    DeadFuncElimResult {
        functions_removed: removed,
    }
}

/// Convenience: build a roots vector from `IrFunction::is_entry`.
pub fn roots_from_is_entry(module: &LpirModule) -> Vec<FuncId> {
    module
        .functions
        .iter()
        .filter(|(_, f)| f.is_entry)
        .map(|(&id, _)| id)
        .collect()
}

/// Convenience: build a roots vector by function name (silently skips unknown names).
pub fn roots_by_name(module: &LpirModule, names: &[&str]) -> Vec<FuncId> {
    let mut out = Vec::with_capacity(names.len());
    for &name in names {
        if let Some((&id, _)) = module.functions.iter().find(|(_, f)| f.name == name) {
            out.push(id);
        }
    }
    out
}
