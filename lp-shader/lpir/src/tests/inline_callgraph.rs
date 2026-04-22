//! Tests for [`crate::inline::callgraph`].

use alloc::string::String;
use alloc::vec;

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::inline::callgraph::{self, CallGraph};
use crate::lpir_module::VMCTX_VREG;
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId, IrType};

fn assert_sorted_dedup_eq(v: &[FuncId], expected: &[FuncId]) {
    assert_eq!(v, expected);
}

#[test]
fn leaf() {
    let mut mb = ModuleBuilder::new();
    let mut f = FunctionBuilder::new("leaf", &[IrType::I32]);
    let p = f.add_param(IrType::I32);
    let tmp = f.alloc_vreg(IrType::I32);
    f.push(LpirOp::IconstI32 { dst: tmp, value: 0 });
    f.push_return(&[p]);
    mb.add_function(f.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(cyclic.is_empty());
    assert_eq!(topo, vec![FuncId(0)]);
    assert!(
        g.callees_of
            .get(&FuncId(0))
            .map(|v| v.is_empty())
            .unwrap_or(true)
    );
}

#[test]
fn linear_chain_a_b_c() {
    let mut mb = ModuleBuilder::new();
    // C: id 0
    let mut c = FunctionBuilder::new("c", &[IrType::I32]);
    let _pc = c.add_param(IrType::I32);
    let r = c.alloc_vreg(IrType::I32);
    c.push(LpirOp::IconstI32 { dst: r, value: 7 });
    c.push_return(&[r]);
    mb.add_function(c.finish());
    let id_c = FuncId(0);

    // B: id 1 — calls C
    let mut b = FunctionBuilder::new("b", &[IrType::I32]);
    let pb = b.add_param(IrType::I32);
    let out = b.alloc_vreg(IrType::I32);
    b.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pb],
        core::slice::from_ref(&out),
    );
    b.push_return(&[out]);
    mb.add_function(b.finish());
    let id_b = FuncId(1);

    // A: id 2 — calls B
    let mut a = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a.add_param(IrType::I32);
    let out = a.alloc_vreg(IrType::I32);
    a.push_call(
        CalleeRef::Local(id_b),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&out),
    );
    a.push_return(&[out]);
    mb.add_function(a.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(cyclic.is_empty());
    assert_eq!(topo, vec![id_c, id_b, FuncId(2)]);
    assert_sorted_dedup_eq(&g.callees_of[&FuncId(2)], &[id_b]);
    assert_sorted_dedup_eq(&g.callees_of[&id_b], &[id_c]);
}

#[test]
fn diamond_a_bc_d() {
    let mut mb = ModuleBuilder::new();
    // D: 0
    let mut d_fn = FunctionBuilder::new("d", &[IrType::I32]);
    let pd = d_fn.add_param(IrType::I32);
    d_fn.push_return(&[pd]);
    mb.add_function(d_fn.finish());
    let id_d = FuncId(0);

    // B: 1 → D
    let mut b_fn = FunctionBuilder::new("b", &[IrType::I32]);
    let pb = b_fn.add_param(IrType::I32);
    let ob = b_fn.alloc_vreg(IrType::I32);
    b_fn.push_call(
        CalleeRef::Local(id_d),
        &[VMCTX_VREG, pb],
        core::slice::from_ref(&ob),
    );
    b_fn.push_return(&[ob]);
    mb.add_function(b_fn.finish());
    let id_b = FuncId(1);

    // C: 2 → D
    let mut c_fn = FunctionBuilder::new("c", &[IrType::I32]);
    let pc = c_fn.add_param(IrType::I32);
    let oc = c_fn.alloc_vreg(IrType::I32);
    c_fn.push_call(
        CalleeRef::Local(id_d),
        &[VMCTX_VREG, pc],
        core::slice::from_ref(&oc),
    );
    c_fn.push_return(&[oc]);
    mb.add_function(c_fn.finish());
    let id_c = FuncId(2);

    // A: 3 → B, C
    let mut a_fn = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a_fn.add_param(IrType::I32);
    let o1 = a_fn.alloc_vreg(IrType::I32);
    let o2 = a_fn.alloc_vreg(IrType::I32);
    a_fn.push_call(
        CalleeRef::Local(id_b),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o1),
    );
    a_fn.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o2),
    );
    a_fn.push_return(&[o1]);
    mb.add_function(a_fn.finish());
    let id_a = FuncId(3);

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(cyclic.is_empty());
    assert_eq!(topo[0], id_d);
    assert_eq!(topo[3], id_a);
    assert_sorted_dedup_eq(&g.callees_of[&id_a], &[id_b, id_c]);
}

#[test]
fn self_recursive() {
    let mut mb = ModuleBuilder::new();
    let mut f = FunctionBuilder::new("rec", &[IrType::I32]);
    let p = f.add_param(IrType::I32);
    let id = mb.next_local_func_id();
    let out = f.alloc_vreg(IrType::I32);
    f.push_call(
        CalleeRef::Local(id),
        &[VMCTX_VREG, p],
        core::slice::from_ref(&out),
    );
    f.push_return(&[out]);
    mb.add_function(f.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (_topo, cyclic) = callgraph::topo_order(&g, &module);
    assert_eq!(cyclic.len(), 1);
    assert!(cyclic.contains(&FuncId(0)));
}

#[test]
fn mutual_recursion() {
    let mut mb = ModuleBuilder::new();
    let id_a = mb.next_local_func_id();
    let id_b = FuncId(id_a.0 + 1);

    let mut a_fn = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a_fn.add_param(IrType::I32);
    let oa = a_fn.alloc_vreg(IrType::I32);
    a_fn.push_call(
        CalleeRef::Local(id_b),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&oa),
    );
    a_fn.push_return(&[oa]);
    mb.add_function(a_fn.finish());

    let mut b_fn = FunctionBuilder::new("b", &[IrType::I32]);
    let pb = b_fn.add_param(IrType::I32);
    let ob = b_fn.alloc_vreg(IrType::I32);
    b_fn.push_call(
        CalleeRef::Local(id_a),
        &[VMCTX_VREG, pb],
        core::slice::from_ref(&ob),
    );
    b_fn.push_return(&[ob]);
    mb.add_function(b_fn.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(topo.is_empty());
    assert_eq!(cyclic.len(), 2);
}

#[test]
fn recursion_with_acyclic_tail() {
    let mut mb = ModuleBuilder::new();
    // C leaf: 0
    let mut c_fn = FunctionBuilder::new("c", &[IrType::I32]);
    let pc = c_fn.add_param(IrType::I32);
    c_fn.push_return(&[pc]);
    mb.add_function(c_fn.finish());
    let id_c = FuncId(0);

    let id_a = FuncId(1);
    let id_b = FuncId(2);

    // A: calls B and C (B added after A)
    let mut a_fn = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a_fn.add_param(IrType::I32);
    let o1 = a_fn.alloc_vreg(IrType::I32);
    let o2 = a_fn.alloc_vreg(IrType::I32);
    a_fn.push_call(
        CalleeRef::Local(id_b),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o1),
    );
    a_fn.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o2),
    );
    a_fn.push_return(&[o1]);
    mb.add_function(a_fn.finish());

    // B: calls A
    let mut b_fn = FunctionBuilder::new("b", &[IrType::I32]);
    let pb = b_fn.add_param(IrType::I32);
    let ob = b_fn.alloc_vreg(IrType::I32);
    b_fn.push_call(
        CalleeRef::Local(id_a),
        &[VMCTX_VREG, pb],
        core::slice::from_ref(&ob),
    );
    b_fn.push_return(&[ob]);
    mb.add_function(b_fn.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(cyclic.contains(&id_a) && cyclic.contains(&id_b));
    assert!(!cyclic.contains(&id_c));
    assert_eq!(topo, vec![id_c]);
}

#[test]
fn import_only_callee() {
    let mut mb = ModuleBuilder::new();
    let imp = mb.add_import(crate::lpir_module::ImportDecl {
        module_name: String::from("m"),
        func_name: String::from("f"),
        param_types: alloc::vec![IrType::I32],
        return_types: alloc::vec![IrType::I32],
        lpfn_glsl_params: None,
        needs_vmctx: true,
    });
    let mut f = FunctionBuilder::new("a", &[IrType::I32]);
    let p = f.add_param(IrType::I32);
    let o = f.alloc_vreg(IrType::I32);
    f.push_call(imp, &[VMCTX_VREG, p], core::slice::from_ref(&o));
    f.push_return(&[o]);
    mb.add_function(f.finish());

    let module = mb.finish();
    let g = callgraph::build(&module);
    let (topo, cyclic) = callgraph::topo_order(&g, &module);
    assert!(cyclic.is_empty());
    assert_eq!(topo, vec![FuncId(0)]);
    assert!(
        g.callees_of
            .get(&FuncId(0))
            .map(|v| v.is_empty())
            .unwrap_or(true)
    );
}

#[test]
fn multiple_call_sites_same_callee() {
    let mut mb = ModuleBuilder::new();
    let mut callee = FunctionBuilder::new("c", &[IrType::I32]);
    let pc = callee.add_param(IrType::I32);
    callee.push_return(&[pc]);
    mb.add_function(callee.finish());
    let id_c = FuncId(0);

    let mut a = FunctionBuilder::new("a", &[IrType::I32]);
    let pa = a.add_param(IrType::I32);
    let o1 = a.alloc_vreg(IrType::I32);
    let o2 = a.alloc_vreg(IrType::I32);
    a.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o1),
    );
    a.push_call(
        CalleeRef::Local(id_c),
        &[VMCTX_VREG, pa],
        core::slice::from_ref(&o2),
    );
    a.push_return(&[o1]);
    mb.add_function(a.finish());

    let module = mb.finish();
    let g: CallGraph = callgraph::build(&module);
    assert_eq!(g.callees_of[&FuncId(1)], vec![id_c]);
    let sites = &g.call_sites_of[&FuncId(1)];
    assert_eq!(sites.len(), 2);
    assert_eq!(sites[0].1, id_c);
    assert_eq!(sites[1].1, id_c);
    assert_ne!(sites[0].0, sites[1].0);
}
