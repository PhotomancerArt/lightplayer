//! Tests for [`crate::dead_func_elim`].

use alloc::string::String;
use alloc::vec;

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::dead_func_elim::{dead_func_elim, roots_by_name, roots_from_is_entry};
use crate::lpir_module::{ImportDecl, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::print::print_module;
use crate::types::{CalleeRef, FuncId, IrType};
use crate::validate::validate_module;

#[test]
fn removes_unreachable_leaf() {
    let mut mb = ModuleBuilder::new();
    let mut dead_helper = FunctionBuilder::new("dead_helper", &[IrType::I32]);
    let _ = dead_helper.add_param(IrType::I32);
    let v = dead_helper.alloc_vreg(IrType::I32);
    dead_helper.push(LpirOp::IconstI32 { dst: v, value: 42 });
    dead_helper.push_return(&[v]);
    mb.add_function(dead_helper.finish());

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let _p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push(LpirOp::IconstI32 { dst: o, value: 0 });
    main.push_return(&[o]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let roots = roots_by_name(&module, &["main"]);
    let r = dead_func_elim(&mut module, &roots);
    assert_eq!(r.functions_removed, 1);
    assert_eq!(module.function_count(), 1);
    assert!(module.functions.values().all(|f| f.name == "main"));
    validate_module(&module).unwrap();
}

#[test]
fn keeps_transitively_reachable() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _ = c.add_param(IrType::I32);
    let v = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: v, value: 1 });
    c.push_return(&[v]);
    mb.add_function(c.finish());
    let id_c = FuncId(0);

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
    let id_b = FuncId(1);

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
    let id_a = FuncId(2);

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push_call(
        CalleeRef::Local(id_a),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    main.push_return(&[o]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let roots = roots_by_name(&module, &["main"]);
    let n = module.function_count();
    let r = dead_func_elim(&mut module, &roots);
    assert_eq!(r.functions_removed, 0);
    assert_eq!(module.function_count(), n);
    validate_module(&module).unwrap();
}

#[test]
fn removes_unreachable_cycle() {
    let mut mb = ModuleBuilder::new();
    let mut a = FunctionBuilder::new("a", &[IrType::I32]);
    let p = a.add_param(IrType::I32);
    let o = a.alloc_vreg(IrType::I32);
    a.push_call(
        CalleeRef::Local(FuncId(1)),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    a.push_return(&[o]);
    let mut b = FunctionBuilder::new("b", &[IrType::I32]);
    let p = b.add_param(IrType::I32);
    let o = b.alloc_vreg(IrType::I32);
    b.push_call(
        CalleeRef::Local(FuncId(0)),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    b.push_return(&[o]);
    mb.add_function(a.finish());
    mb.add_function(b.finish());

    let mut module = mb.finish();
    let r = dead_func_elim(&mut module, &[]);
    assert_eq!(r.functions_removed, 2);
    assert!(module.functions.is_empty());
    validate_module(&module).unwrap();
}

#[test]
fn multiple_roots() {
    let mut mb = ModuleBuilder::new();
    for name in ["h_main", "h_init", "dead_orphan"] {
        let mut f = FunctionBuilder::new(name, &[IrType::I32]);
        let _ = f.add_param(IrType::I32);
        let o = f.alloc_vreg(IrType::I32);
        f.push(LpirOp::IconstI32 { dst: o, value: 0 });
        f.push_return(&[o]);
        mb.add_function(f.finish());
    }
    let h_main = FuncId(0);
    let h_init = FuncId(1);

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push_call(
        CalleeRef::Local(h_main),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    main.push_return(&[o]);
    mb.add_function(main.finish());

    let mut shader_init = FunctionBuilder::new("__shader_init", &[IrType::I32]);
    let p = shader_init.add_param(IrType::I32);
    let o = shader_init.alloc_vreg(IrType::I32);
    shader_init.push_call(
        CalleeRef::Local(h_init),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    shader_init.push_return(&[o]);
    mb.add_function(shader_init.finish());

    let mut module = mb.finish();
    let roots = roots_by_name(&module, &["main", "__shader_init"]);
    let r = dead_func_elim(&mut module, &roots);
    assert_eq!(r.functions_removed, 1);
    assert_eq!(module.function_count(), 4);
    assert!(!module.functions.values().any(|f| f.name == "dead_orphan"));
    validate_module(&module).unwrap();
}

#[test]
fn no_roots_removes_everything() {
    let mut mb = ModuleBuilder::new();
    for name in ["a", "b"] {
        let mut f = FunctionBuilder::new(name, &[IrType::I32]);
        let _ = f.add_param(IrType::I32);
        let o = f.alloc_vreg(IrType::I32);
        f.push(LpirOp::IconstI32 { dst: o, value: 0 });
        f.push_return(&[o]);
        mb.add_function(f.finish());
    }
    let mut module = mb.finish();
    let r = dead_func_elim(&mut module, &[]);
    assert_eq!(r.functions_removed, 2);
    assert!(module.functions.is_empty());
    validate_module(&module).unwrap();
}

#[test]
fn roots_from_is_entry_picks_marked() {
    let mut mb = ModuleBuilder::new();
    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    main.set_entry();
    let _ = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push(LpirOp::IconstI32 { dst: o, value: 0 });
    main.push_return(&[o]);
    mb.add_function(main.finish());
    let mut other = FunctionBuilder::new("other", &[IrType::I32]);
    let _ = other.add_param(IrType::I32);
    let o = other.alloc_vreg(IrType::I32);
    other.push(LpirOp::IconstI32 { dst: o, value: 1 });
    other.push_return(&[o]);
    mb.add_function(other.finish());

    let module = mb.finish();
    let roots = roots_from_is_entry(&module);
    assert_eq!(roots, vec![FuncId(0)]);
}

#[test]
fn roots_by_name_skips_unknown() {
    let mut mb = ModuleBuilder::new();
    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let _ = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push(LpirOp::IconstI32 { dst: o, value: 0 });
    main.push_return(&[o]);
    mb.add_function(main.finish());
    let module = mb.finish();
    let roots = roots_by_name(&module, &["main", "missing"]);
    assert_eq!(roots, vec![FuncId(0)]);
}

#[test]
fn noop_when_all_reachable() {
    let mut mb = ModuleBuilder::new();
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _ = c.add_param(IrType::I32);
    let v = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: v, value: 1 });
    c.push_return(&[v]);
    mb.add_function(c.finish());
    let id_c = FuncId(0);

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
    let id_b = FuncId(1);

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
    let id_a = FuncId(2);

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push_call(
        CalleeRef::Local(id_a),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&o),
    );
    main.push_return(&[o]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let before = print_module(&module);
    let roots = roots_by_name(&module, &["main"]);
    let r = dead_func_elim(&mut module, &roots);
    assert_eq!(r.functions_removed, 0);
    assert_eq!(print_module(&module), before);
    validate_module(&module).unwrap();
}

#[test]
fn import_calls_dont_count_as_local_edges() {
    let mut mb = ModuleBuilder::new();
    let imp = mb.add_import(ImportDecl {
        module_name: String::from("g"),
        func_name: String::from("sin"),
        param_types: vec![IrType::F32],
        return_types: vec![IrType::F32],
        lpfn_glsl_params: None,
        needs_vmctx: true,
    });

    let mut only_import = FunctionBuilder::new("only_import_caller", &[IrType::F32]);
    let p = only_import.add_param(IrType::F32);
    let out = only_import.alloc_vreg(IrType::F32);
    only_import.push_call(imp, &[VMCTX_VREG, p], &[out]);
    only_import.push_return(&[out]);
    mb.add_function(only_import.finish());

    let mut main = FunctionBuilder::new("main", &[IrType::I32]);
    let _p = main.add_param(IrType::I32);
    let o = main.alloc_vreg(IrType::I32);
    main.push(LpirOp::IconstI32 { dst: o, value: 0 });
    main.push_return(&[o]);
    mb.add_function(main.finish());

    let mut module = mb.finish();
    let roots = roots_by_name(&module, &["main"]);
    let r = dead_func_elim(&mut module, &roots);
    assert_eq!(r.functions_removed, 1);
    assert_eq!(module.function_count(), 1);
    assert!(module.functions.values().all(|f| f.name == "main"));
    validate_module(&module).unwrap();
}
