//! Tests for [`crate::inline::heuristic::should_inline`] and budget behavior via [`crate::inline_module`].

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::inline::heuristic::{BudgetReason, Decision, should_inline};
use crate::lpir_module::VMCTX_VREG;
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, IrType};
use crate::{InlineConfig, InlineMode, inline_module};

#[test]
fn mode_never_is_skip_mode() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Never;
    assert_eq!(should_inline(1, 99, 0, &c), Decision::SkipMode);
}

#[test]
fn mode_always_inlines_huge_callee() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Always;
    c.small_func_threshold = 1;
    assert_eq!(should_inline(10_000, 1, 1_000_000, &c), Decision::Inline);
}

#[test]
fn auto_skips_large_multi_site() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Auto;
    c.small_func_threshold = 5;
    c.always_inline_single_site = true;
    assert!(matches!(
        should_inline(10, 2, 0, &c),
        Decision::SkipTooLarge { .. }
    ));
}

#[test]
fn auto_inlines_large_single_site() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Auto;
    c.small_func_threshold = 5;
    c.always_inline_single_site = true;
    assert_eq!(should_inline(10, 1, 0, &c), Decision::Inline);
}

#[test]
fn auto_skips_large_single_site_when_disabled() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Auto;
    c.small_func_threshold = 5;
    c.always_inline_single_site = false;
    assert!(matches!(
        should_inline(10, 1, 0, &c),
        Decision::SkipTooLarge { .. }
    ));
}

#[test]
fn max_growth_budget_per_callee() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Always;
    c.max_growth_budget = Some(20);
    assert!(matches!(
        should_inline(11, 2, 0, &c),
        Decision::SkipBudget {
            reason: BudgetReason::MaxGrowth,
            ..
        }
    ));
    assert_eq!(should_inline(10, 2, 0, &c), Decision::Inline);
}

#[test]
fn module_op_budget_on_should_inline() {
    let mut c = InlineConfig::default();
    c.mode = InlineMode::Always;
    c.module_op_budget = Some(15);
    assert!(matches!(
        should_inline(5, 2, 6, &c),
        Decision::SkipBudget {
            reason: BudgetReason::ModuleTotal,
            ..
        }
    ));
}

#[test]
fn module_op_budget_hit_inline_module() {
    let mut mb = ModuleBuilder::new();
    let mut leaf = FunctionBuilder::new("leaf", &[IrType::I32]);
    let p = leaf.add_param(IrType::I32);
    let v = leaf.alloc_vreg(IrType::I32);
    leaf.push(LpirOp::IconstI32 { dst: v, value: 1 });
    leaf.push_return(&[p]);
    let cref = mb.add_function(leaf.finish());

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let pm = main.add_param(IrType::I32);
    let out = main.alloc_vreg(IrType::I32);
    main.push_call(cref, &[VMCTX_VREG, pm], &[out]);
    main.push_return(&[out]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let mut cfg = InlineConfig::default();
    cfg.mode = InlineMode::Always;
    cfg.module_op_budget = Some(3);
    let r = inline_module(&mut module, &cfg);
    assert!(r.budget_exceeded);
}

#[test]
fn debug_decisions_use_mode_never_no_inline() {
    let mut mb = ModuleBuilder::new();
    let mut leaf = FunctionBuilder::new("leaf", &[IrType::I32]);
    let _ = leaf.add_param(IrType::I32);
    let v = leaf.alloc_vreg(IrType::I32);
    leaf.push(LpirOp::IconstI32 { dst: v, value: 7 });
    leaf.push_return(&[v]);
    let cref = mb.add_function(leaf.finish());
    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push_call(cref, &[VMCTX_VREG, p], &[o]);
    main.push_return(&[o]);
    mb.add_function(main.finish());
    let mut module = mb.finish();
    let mut cfg = InlineConfig::default();
    cfg.mode = InlineMode::Never;
    let r = inline_module(&mut module, &cfg);
    assert_eq!(r.call_sites_replaced, 0);
    assert_eq!(r.functions_inlined, 0);
}

#[test]
fn max_growth_still_allows_other_callees_orchestration() {
    let mut mb = ModuleBuilder::new();
    let mut always_small = FunctionBuilder::new("small", &[IrType::I32]);
    let ps = always_small.add_param(IrType::I32);
    always_small.push_return(&[ps]);
    let small_ref = mb.add_function(always_small.finish());

    let mut huge = FunctionBuilder::new("huge", &[IrType::I32]);
    let ph = huge.add_param(IrType::I32);
    for _ in 0..40 {
        let t = huge.alloc_vreg(IrType::I32);
        huge.push(LpirOp::IconstI32 { dst: t, value: 0 });
    }
    huge.push_return(&[ph]);
    let huge_ref = mb.add_function(huge.finish());
    let id_huge = match huge_ref {
        CalleeRef::Local(id) => id,
        _ => unreachable!(),
    };

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let pm = main.add_param(IrType::I32);
    let o1 = main.alloc_vreg(IrType::I32);
    let o2 = main.alloc_vreg(IrType::I32);
    main.push_call(huge_ref, &[VMCTX_VREG, pm], &[o1]);
    main.push_call(small_ref, &[VMCTX_VREG, pm], &[o2]);
    main.push_return(&[o2]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let mut cfg = InlineConfig::default();
    cfg.mode = InlineMode::Always;
    cfg.max_growth_budget = Some(30);
    let r = inline_module(&mut module, &cfg);
    assert_eq!(r.functions_inlined, 1);
    let still_calls_huge = module.functions.values().any(|f| {
        f.body.iter().any(
            |op| matches!(op, LpirOp::Call { callee: CalleeRef::Local(id), .. } if *id == id_huge),
        )
    });
    assert!(still_calls_huge);
}
