//! Local call graph for module-level bottom-up passes.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::lpir_module::LpirModule;
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId};

pub(crate) struct CallGraph {
    /// `callees_of[caller]` = sorted, deduplicated list of local [`FuncId`]s called.
    pub callees_of: BTreeMap<FuncId, Vec<FuncId>>,
    /// `callers_of[callee]` = sorted, deduplicated list of local [`FuncId`]s calling it.
    pub callers_of: BTreeMap<FuncId, Vec<FuncId>>,
    /// Per caller: `(op_index, callee)` in body order (one entry per call site).
    pub call_sites_of: BTreeMap<FuncId, Vec<(usize, FuncId)>>,
}

pub(crate) fn build(module: &LpirModule) -> CallGraph {
    let mut callees_raw: BTreeMap<FuncId, BTreeSet<FuncId>> = BTreeMap::new();
    let mut callers_raw: BTreeMap<FuncId, BTreeSet<FuncId>> = BTreeMap::new();
    let mut call_sites_of: BTreeMap<FuncId, Vec<(usize, FuncId)>> = BTreeMap::new();

    for (&caller_id, func) in &module.functions {
        for (idx, op) in func.body.iter().enumerate() {
            if let LpirOp::Call {
                callee: CalleeRef::Local(callee_id),
                ..
            } = op
            {
                callees_raw
                    .entry(caller_id)
                    .or_default()
                    .insert(*callee_id);
                callers_raw
                    .entry(*callee_id)
                    .or_default()
                    .insert(caller_id);
                call_sites_of
                    .entry(caller_id)
                    .or_default()
                    .push((idx, *callee_id));
            }
        }
    }

    let callees_of = callees_raw
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    let callers_of = callers_raw
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();

    CallGraph {
        callees_of,
        callers_of,
        call_sites_of,
    }
}

/// Kahn topological order (leaves / callees first). Remaining nodes form cycles.
/// `module` supplies every [`FuncId`] so isolated functions (no calls / not called) participate.
pub(crate) fn topo_order(g: &CallGraph, module: &LpirModule) -> (Vec<FuncId>, BTreeSet<FuncId>) {
    let mut in_degree: BTreeMap<FuncId, usize> = BTreeMap::new();
    for &f in module.functions.keys() {
        let d = g.callees_of.get(&f).map(|v| v.len()).unwrap_or(0);
        in_degree.insert(f, d);
    }

    let mut queue: BTreeSet<FuncId> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&f, _)| f)
        .collect();

    let mut topo = Vec::new();
    while let Some(gid) = queue.iter().next().copied() {
        queue.remove(&gid);
        topo.push(gid);
        if let Some(callers) = g.callers_of.get(&gid) {
            for &caller in callers {
                if let Some(deg) = in_degree.get_mut(&caller) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.insert(caller);
                    }
                }
            }
        }
    }

    let cyclic: BTreeSet<FuncId> = in_degree
        .into_iter()
        .filter(|(_, d)| *d > 0)
        .map(|(f, _)| f)
        .collect();

    (topo, cyclic)
}
