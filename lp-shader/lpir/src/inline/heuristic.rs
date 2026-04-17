//! Size / budget gating for inlining.

use crate::compiler_config::{InlineConfig, InlineMode};
use crate::lpir_module::IrFunction;

pub(crate) fn func_weight(func: &IrFunction) -> usize {
    func.body.len()
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
