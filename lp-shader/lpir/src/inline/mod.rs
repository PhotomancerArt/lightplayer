//! LPIR inlining pass — bottom-up, never deletes functions, structural
//! offset recompute. See docs/plans/2026-04-17-lpir-inliner-stage-iii.

pub(crate) mod callgraph;
pub(crate) mod heuristic;
mod offsets;
pub(crate) mod remap;
pub(crate) mod splice;

pub(crate) use offsets::recompute_offsets;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::InlineConfig;
use crate::inline::callgraph::CallGraph;
use crate::inline::heuristic::{BudgetReason, Decision};
use crate::lpir_module::LpirModule;
use crate::types::FuncId;

/// Counters and flags returned by [`inline_module`].
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineResult {
    /// Distinct callees inlined into at least one caller this run.
    pub functions_inlined: usize,
    /// `Call` sites replaced with callee bodies.
    pub call_sites_replaced: usize,
    /// Functions on a local call cycle (skipped; bodies unchanged).
    pub functions_skipped_recursive: usize,
    /// True when `InlineConfig::module_op_budget` is exceeded and the pass stops early.
    pub budget_exceeded: bool,
}

fn total_op_count(module: &LpirModule) -> usize {
    module.functions.values().map(|f| f.body.len()).sum()
}

fn call_sites_for_callee(graph: &CallGraph, callee_id: FuncId) -> Vec<(FuncId, usize)> {
    let mut out = Vec::new();
    for (&caller_id, sites) in &graph.call_sites_of {
        for &(op_idx, c) in sites {
            if c == callee_id {
                out.push((caller_id, op_idx));
            }
        }
    }
    out
}

fn group_by_caller_desc(sites: &[(FuncId, usize)]) -> Vec<(FuncId, Vec<usize>)> {
    let mut map: BTreeMap<FuncId, Vec<usize>> = BTreeMap::new();
    for &(caller, idx) in sites {
        map.entry(caller).or_default().push(idx);
    }
    let mut out: Vec<(FuncId, Vec<usize>)> = map.into_iter().collect();
    for (_, indices) in &mut out {
        indices.sort_by(|a, b| b.cmp(a));
    }
    out
}

/// Bottom-up local inlining pass: mutates `module` in place, never removes functions.
pub fn inline_module(module: &mut LpirModule, config: &InlineConfig) -> InlineResult {
    let graph = callgraph::build(module);
    let (topo, cyclic) = callgraph::topo_order(&graph, module);

    let mut result = InlineResult {
        functions_skipped_recursive: cyclic.len(),
        ..Default::default()
    };
    for &cyc in &cyclic {
        log::debug!("inline: skip recursive func={cyc:?}");
    }

    let mut current_op_count = total_op_count(module);
    let mut inlined_callees = BTreeSet::new();
    let mut mutated_callers = BTreeSet::new();

    'outer: for callee_id in topo {
        if cyclic.contains(&callee_id) {
            continue;
        }
        let Some(callee_fn) = module.functions.get(&callee_id) else {
            continue;
        };
        let weight = heuristic::func_weight(callee_fn);
        let sites = call_sites_for_callee(&graph, callee_id);
        if sites.is_empty() {
            continue;
        }

        match heuristic::should_inline(weight, sites.len(), current_op_count, config) {
            Decision::Inline => {
                log::debug!(
                    "inline: callee={:?} weight={} sites={} module_ops={} decision=inline",
                    callee_id,
                    weight,
                    sites.len(),
                    current_op_count
                );
                let by_caller = group_by_caller_desc(&sites);
                let callee = module.functions.remove(&callee_id).expect("topo callee");
                for (caller_id, indices) in by_caller {
                    let caller = module.functions.get_mut(&caller_id).expect("caller");
                    for op_idx in indices {
                        splice::inline_call_site(caller, &callee, op_idx);
                        result.call_sites_replaced += 1;
                    }
                    mutated_callers.insert(caller_id);
                }
                module.functions.insert(callee_id, callee);
                inlined_callees.insert(callee_id);
                current_op_count = total_op_count(module);
            }
            Decision::SkipTooLarge { weight, threshold } => {
                log::debug!(
                    "inline: callee={callee_id:?} skip too_large weight={weight} threshold={threshold}"
                );
            }
            Decision::SkipBudget {
                projected,
                budget,
                reason,
            } => {
                log::debug!(
                    "inline: callee={callee_id:?} skip budget projected={projected} budget={budget} reason={reason:?}"
                );
                if matches!(reason, BudgetReason::ModuleTotal) {
                    result.budget_exceeded = true;
                    break 'outer;
                }
            }
            Decision::SkipMode => {
                log::debug!("inline: callee={callee_id:?} skip mode=Never");
            }
        }
    }

    for caller_id in mutated_callers {
        let f = module
            .functions
            .get_mut(&caller_id)
            .expect("mutated caller");
        recompute_offsets(&mut f.body);
        f.body.shrink_to_fit();
    }

    result.functions_inlined = inlined_callees.len();
    log::info!(
        "inline: done inlined={} sites={} skipped_recursive={} budget_exceeded={}",
        result.functions_inlined,
        result.call_sites_replaced,
        result.functions_skipped_recursive,
        result.budget_exceeded
    );
    result
}
