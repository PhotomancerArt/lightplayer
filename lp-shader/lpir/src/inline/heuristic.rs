//! Size / budget gating for inlining.

use crate::compiler_config::{InlineConfig, InlineMode};
use crate::lpir_module::IrFunction;
use crate::lpir_op::LpirOp;

pub(crate) fn func_weight(func: &IrFunction) -> usize {
    func.body.len()
}

/// Which candidate [`weight`] function to use (M3.1 tuning; not wired to [`func_weight`] yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightKind {
    BodyLen,
    MarkersZero,
    HeavyBias,
}

/// Dispatch for candidate inline size metrics.
pub fn weight(kind: WeightKind, func: &IrFunction) -> usize {
    match kind {
        WeightKind::BodyLen => weight_body_len(func),
        WeightKind::MarkersZero => weight_markers_zero(func),
        WeightKind::HeavyBias => weight_heavy_bias(func),
    }
}

/// Baseline: raw LPIR op count (same as production [`func_weight`] today).
pub fn weight_body_len(func: &IrFunction) -> usize {
    func.body.len()
}

/// Count each op as 1 except structural / pure-marker ops weighted 0 (M3.1 plan):
/// [`LpirOp::IfStart`], [`LpirOp::Else`], [`LpirOp::Continuing`], [`LpirOp::LoopStart`],
/// [`LpirOp::SwitchStart`], [`LpirOp::CaseStart`], [`LpirOp::DefaultStart`], [`LpirOp::End`],
/// [`LpirOp::Block`], [`LpirOp::ExitBlock`], [`LpirOp::Break`], [`LpirOp::Continue`],
/// [`LpirOp::Return`]. Rationale: no standalone RV32 lowering for these; [`LpirOp::Return`]
/// is an epilogue / lifetime boundary for sizing, not a counted “op” in this metric.
pub fn weight_markers_zero(func: &IrFunction) -> usize {
    func.body.iter().map(weight_op_markers_zero).sum()
}

/// Like [`weight_markers_zero`], with extra cost on ops that tend to expand to more
/// machine code or helper calls: [`LpirOp::Call`] (call/return and arg shuffle),
/// [`LpirOp::Memcpy`] (loop-bodied helper), [`LpirOp::Fsqrt`] (multi-cycle / lib helper),
/// and slow div/rem helpers ([`LpirOp::IdivS`], [`LpirOp::IdivU`], [`LpirOp::IremS`],
/// [`LpirOp::IremU`], [`LpirOp::Fdiv`]) for empirical correlation tests.
pub fn weight_heavy_bias(func: &IrFunction) -> usize {
    func.body.iter().map(weight_op_heavy_bias).sum()
}

fn weight_op_markers_zero(op: &LpirOp) -> usize {
    match op {
        LpirOp::IfStart { .. }
        | LpirOp::Else
        | LpirOp::Continuing
        | LpirOp::LoopStart { .. }
        | LpirOp::SwitchStart { .. }
        | LpirOp::CaseStart { .. }
        | LpirOp::DefaultStart { .. }
        | LpirOp::End
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock
        | LpirOp::Break
        | LpirOp::Continue
        | LpirOp::Return { .. } => 0,
        _ => 1,
    }
}

fn weight_op_heavy_bias(op: &LpirOp) -> usize {
    match op {
        LpirOp::IfStart { .. }
        | LpirOp::Else
        | LpirOp::Continuing
        | LpirOp::LoopStart { .. }
        | LpirOp::SwitchStart { .. }
        | LpirOp::CaseStart { .. }
        | LpirOp::DefaultStart { .. }
        | LpirOp::End
        | LpirOp::Block { .. }
        | LpirOp::ExitBlock
        | LpirOp::Break
        | LpirOp::Continue
        | LpirOp::Return { .. } => 0,
        LpirOp::Call { .. } => 5,
        LpirOp::Memcpy { .. } => 4,
        LpirOp::Fsqrt { .. } => 4,
        LpirOp::IdivS { .. }
        | LpirOp::IdivU { .. }
        | LpirOp::IremS { .. }
        | LpirOp::IremU { .. }
        | LpirOp::Fdiv { .. } => 3,
        _ => 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetReason {
    MaxGrowth,
    ModuleTotal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Inline,
    SkipTooLarge {
        weight: usize,
        threshold: usize,
    },
    SkipBudget {
        projected: usize,
        budget: usize,
        reason: BudgetReason,
    },
    SkipMode,
}

pub(crate) fn should_inline(
    callee_weight: usize,
    callsite_count: usize,
    current_module_op_count: usize,
    config: &InlineConfig,
) -> Decision {
    use InlineMode::*;

    if matches!(config.mode, Never) {
        return Decision::SkipMode;
    }

    if matches!(config.mode, Auto) {
        if callee_weight > config.small_func_threshold
            && (callsite_count > 1 || !config.always_inline_single_site)
        {
            return Decision::SkipTooLarge {
                weight: callee_weight,
                threshold: config.small_func_threshold,
            };
        }
    }

    let projected = callee_weight.saturating_mul(callsite_count);
    if let Some(b) = config.max_growth_budget {
        if projected > b {
            return Decision::SkipBudget {
                projected,
                budget: b,
                reason: BudgetReason::MaxGrowth,
            };
        }
    }

    if let Some(b) = config.module_op_budget {
        let projected_total = current_module_op_count.saturating_add(projected);
        if projected_total > b {
            return Decision::SkipBudget {
                projected: projected_total,
                budget: b,
                reason: BudgetReason::ModuleTotal,
            };
        }
    }

    Decision::Inline
}
