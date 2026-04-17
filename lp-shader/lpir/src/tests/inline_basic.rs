//! Tests for [`crate::inline::splice::inline_call_site`] and (Phase 6) [`crate::inline_module`] inliner.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::inline::recompute_offsets;
use crate::inline::splice::inline_call_site;
use crate::interp::{ImportHandler, InterpError, Value, interpret};
use crate::{inline_module, InlineConfig};
use crate::lpir_module::{ImportDecl, LpirModule, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, IrType};
use crate::validate::validate_module;

struct NoImports;

impl ImportHandler for NoImports {
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(String::from("no imports")))
    }
}

struct SinImport;

impl ImportHandler for SinImport {
    fn call(
        &mut self,
        module: &str,
        name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        if module == "g" && name == "sin" {
            let x = args.get(1).and_then(|v| v.as_f32()).unwrap_or(0.0);
            return Ok(vec![Value::F32(libm::sinf(x))]);
        }
        Err(InterpError::Import(String::from("bad import")))
    }
}

fn find_local_call(f: &crate::lpir_module::IrFunction) -> Option<usize> {
    f.body.iter().enumerate().find_map(|(i, o)| {
        matches!(o, LpirOp::Call {
            callee: CalleeRef::Local(_),
            ..
        })
        .then_some(i)
    })
}

fn run_i32(module: &LpirModule, name: &str, args: &[Value]) -> i32 {
    let out = interpret(module, name, args, &mut NoImports).unwrap();
    assert_eq!(out.len(), 1);
    out[0].as_i32().expect("i32")
}

#[test]
fn void_callee() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[]);
    let _ = c.add_param(IrType::I32);
    let s0 = c.alloc_slot(4);
    let base = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::SlotAddr {
        dst: base,
        slot: s0,
    });
    let z = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: z, value: 99 });
    c.push(LpirOp::Store {
        base,
        offset: 0,
        value: z,
    });
    c.push_return(&[]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[]);
    let r = t.alloc_vreg(IrType::I32);
    t.push(LpirOp::IconstI32 { dst: r, value: 0 });
    t.push_return(&[r]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let before = run_i32(&module, "t", &[Value::I32(1)]);
    let caller_id = module.functions.keys().nth(1).copied().expect("caller");
    let callee_fn = module.functions.values().nth(0).expect("callee").clone();
    let idx = find_local_call(module.functions.get(&caller_id).expect("t")).expect("call");
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    let after = run_i32(&module, "t", &[Value::I32(1)]);
    assert_eq!(before, after);
}

#[test]
fn single_return_at_end() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("add1", &[IrType::I32]);
    let a = c.add_param(IrType::I32);
    let one = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: one, value: 1 });
    let r = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::Iadd {
        dst: r,
        lhs: a,
        rhs: one,
    });
    c.push_return(&[r]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let before = run_i32(&module, "t", &[Value::I32(41)]);
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "t", &[Value::I32(41)]), before);
    assert!(
        !module.functions[&caller_id]
            .body
            .iter()
            .any(|o| matches!(o, LpirOp::Block { .. }))
    );
}

#[test]
fn single_return_not_at_end() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("early", &[IrType::I32]);
    let a = c.add_param(IrType::I32);
    c.push_if(a);
    let neg = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::Ineg { dst: neg, src: a });
    c.push_return(&[neg]);
    c.push_else();
    let z = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: z, value: 0 });
    c.push_return(&[z]);
    c.end_if();
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let before = run_i32(&module, "t", &[Value::I32(-5)]);
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "t", &[Value::I32(-5)]), before);
    assert!(
        module.functions[&caller_id]
            .body
            .iter()
            .any(|o| matches!(o, LpirOp::Block { .. }))
    );
}

#[test]
fn multiple_returns() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("two_ret", &[IrType::I32]);
    let a = c.add_param(IrType::I32);
    c.push_if(a);
    let one = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: one, value: 1 });
    c.push_return(&[one]);
    c.push_else();
    let two = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: two, value: 2 });
    c.push_return(&[two]);
    c.end_if();
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let before0 = run_i32(&module, "t", &[Value::I32(0)]);
    let before1 = run_i32(&module, "t", &[Value::I32(7)]);
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "t", &[Value::I32(0)]), before0);
    assert_eq!(run_i32(&module, "t", &[Value::I32(7)]), before1);
}

#[test]
fn nested_call_in_callee() {
    let mut mb = ModuleBuilder::new();
    let imp = mb.add_import(ImportDecl {
        module_name: String::from("g"),
        func_name: String::from("sin"),
        param_types: vec![IrType::F32],
        return_types: vec![IrType::F32],
        lpfn_glsl_params: None,
        needs_vmctx: true,
    });

    let mut c = FunctionBuilder::new("c", &[IrType::F32]);
    let a = c.add_param(IrType::F32);
    let out = c.alloc_vreg(IrType::F32);
    c.push_call(imp, &[VMCTX_VREG, a], &[out]);
    c.push_return(&[out]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::F32]);
    let p = t.add_param(IrType::F32);
    let r = t.alloc_vreg(IrType::F32);
    t.push_call(cref, &[VMCTX_VREG, p], &[r]);
    t.push_return(&[r]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let before = interpret(&module, "t", &[Value::F32(0.3)], &mut SinImport).unwrap();
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    let after = interpret(&module, "t", &[Value::F32(0.3)], &mut SinImport).unwrap();
    assert_eq!(before.len(), after.len());
    assert!((before[0].as_f32().unwrap() - after[0].as_f32().unwrap()).abs() < 1e-5);
}

#[test]
fn mutated_param() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let a = c.add_param(IrType::I32);
    let one = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: one, value: 1 });
    c.push(LpirOp::Iadd {
        dst: a,
        lhs: a,
        rhs: one,
    });
    c.push_return(&[a]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    let copy_count = module.functions[&caller_id]
        .body
        .iter()
        .filter(|o| matches!(o, LpirOp::Copy { .. }))
        .count();
    assert!(copy_count >= 2, "param Copy plus return Copy");
}

#[test]
fn readonly_param() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let a = c.add_param(IrType::I32);
    let r = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::Iadd {
        dst: r,
        lhs: a,
        rhs: a,
    });
    c.push_return(&[r]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    let copy_count = module.functions[&caller_id]
        .body
        .iter()
        .filter(|o| matches!(o, LpirOp::Copy { .. }))
        .count();
    assert_eq!(copy_count, 1, "only return lowering Copy, no param preamble");
}

#[test]
fn vmctx_propagation() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _ = c.add_param(IrType::I32);
    let r = c.alloc_vreg(IrType::I32);
    // `Load` from VMCTX is not interpreted meaningfully in the harness, but it
    // keeps `v0` as the base pointer through validation + remap.
    c.push(LpirOp::Load {
        dst: r,
        base: VMCTX_VREG,
        offset: 0,
    });
    c.push_return(&[r]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    validate_module(&module).unwrap();
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    assert!(
        module.functions[&caller_id]
            .body
            .iter()
            .any(|o| matches!(o, LpirOp::Load { base: VMCTX_VREG, .. }))
    );
}

#[test]
fn slot_remap() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _ = c.add_param(IrType::I32);
    let s0 = c.alloc_slot(4);
    let s1 = c.alloc_slot(4);
    let a0 = c.alloc_vreg(IrType::I32);
    let a1 = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::SlotAddr { dst: a0, slot: s0 });
    c.push(LpirOp::SlotAddr { dst: a1, slot: s1 });
    let forty_two = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 {
        dst: forty_two,
        value: 42,
    });
    c.push(LpirOp::Store {
        base: a0,
        offset: 0,
        value: forty_two,
    });
    let r = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::Load {
        dst: r,
        base: a0,
        offset: 0,
    });
    c.push_return(&[r]);
    let cref = mb.add_function(c.finish());

    let mut t = FunctionBuilder::new("t", &[IrType::I32]);
    for _ in 0..3 {
        t.alloc_slot(1);
    }
    let p = t.add_param(IrType::I32);
    let out = t.alloc_vreg(IrType::I32);
    t.push_call(cref, &[VMCTX_VREG, p], &[out]);
    t.push_return(&[out]);
    mb.add_function(t.finish());

    let mut module = mb.finish();
    let caller_id = *module.functions.keys().nth(1).unwrap();
    let callee_fn = module.functions.values().nth(0).unwrap().clone();
    let idx = find_local_call(module.functions.get(&caller_id).unwrap()).unwrap();
    {
        let caller = module.functions.get_mut(&caller_id).unwrap();
        inline_call_site(caller, &callee_fn, idx);
        recompute_offsets(&mut caller.body);
    }
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "t", &[Value::I32(0)]), 42);
    let slots: Vec<_> = module.functions[&caller_id]
        .body
        .iter()
        .filter_map(|o| {
            if let LpirOp::SlotAddr { slot, .. } = o {
                Some(slot.0)
            } else {
                None
            }
        })
        .collect();
    assert!(slots.contains(&3));
    assert!(slots.contains(&4));
}

#[test]
fn leaf_inlined_into_caller() {
    let mut mb = ModuleBuilder::new();
    let mut leaf = FunctionBuilder::new("leaf", &[IrType::I32]);
    let _ = leaf.add_param(IrType::I32);
    let v = leaf.alloc_vreg(IrType::I32);
    leaf.push(LpirOp::IconstI32 { dst: v, value: 99 });
    leaf.push_return(&[v]);
    let cref = mb.add_function(leaf.finish());

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let p = main.add_param(IrType::I32);
    let out = main.alloc_vreg(IrType::I32);
    main.push_call(cref, &[VMCTX_VREG, p], &[out]);
    main.push_return(&[out]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let want = run_i32(&module, "main", &[Value::I32(0)]);
    let cfg = InlineConfig::default();
    let r = inline_module(&mut module, &cfg);
    assert!(r.call_sites_replaced >= 1);
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "main", &[Value::I32(0)]), want);
}

#[test]
fn chain_inlined_bottom_up() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _ = c.add_param(IrType::I32);
    let v = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: v, value: 1 });
    c.push_return(&[v]);
    mb.add_function(c.finish());
    let id_c = crate::types::FuncId(0);

    let mut b = FunctionBuilder::new("b", &[IrType::I32]);
    let pb = b.add_param(IrType::I32);
    let o = b.alloc_vreg(IrType::I32);
    b.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pb],
        core::slice::from_ref(&o),
    );
    b.push_return(&[o]);
    mb.add_function(b.finish());
    let id_b = crate::types::FuncId(1);

    let mut a = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a.add_param(IrType::I32);
    let o = a.alloc_vreg(IrType::I32);
    a.push_call(
        CalleeRef::Local(id_b),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o),
    );
    a.push_return(&[o]);
    mb.add_function(a.finish());

    let mut module = mb.finish();
    let want = run_i32(&module, "a", &[Value::I32(0)]);
    let r = inline_module(&mut module, &InlineConfig::default());
    assert!(r.call_sites_replaced >= 2);
    validate_module(&module).unwrap();
    assert_eq!(run_i32(&module, "a", &[Value::I32(0)]), want);
}

#[test]
fn recursive_skipped() {
    let mut mb = ModuleBuilder::new();
    let id = mb.next_local_func_id();
    let mut a = FunctionBuilder::new("a", &[IrType::I32]);
    let p = a.add_param(IrType::I32);
    let o = a.alloc_vreg(IrType::I32);
    a.push_call(
        CalleeRef::Local(id),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    a.push_return(&[o]);
    mb.add_function(a.finish());

    let mut module = mb.finish();
    let fid = *module.functions.keys().next().unwrap();
    let len_before = module.functions[&fid].body.len();
    let r = inline_module(&mut module, &InlineConfig::default());
    assert_eq!(r.functions_skipped_recursive, 1);
    assert_eq!(r.call_sites_replaced, 0);
    validate_module(&module).unwrap();
    assert_eq!(module.functions[&fid].body.len(), len_before);
    assert!(
        find_local_call(&module.functions[&fid]).is_some(),
        "self-call still present"
    );
}

