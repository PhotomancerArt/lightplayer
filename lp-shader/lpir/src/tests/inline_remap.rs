//! Tests for [`crate::inline::remap::build_remap`] and [`crate::inline::remap::remap_op`].

use alloc::string::String;
use alloc::vec;

use crate::builder::FunctionBuilder;
use crate::inline::remap::{build_remap, remap_op, scan_param_writes};
use crate::lpir_module::{IrFunction, SlotDecl, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{IrType, VReg};

#[test]
fn alias_for_readonly_param() {
    let mut b = FunctionBuilder::new("c", &[IrType::I32]);
    let a = b.add_param(IrType::I32);
    b.push_return(&[a]);
    let callee = b.finish();
    let pw = scan_param_writes(&callee);
    let mut caller = FunctionBuilder::new("caller", &[IrType::I32]).finish();
    let arg = VReg(5);
    let r = build_remap(
        &mut caller,
        &callee,
        &[VMCTX_VREG, arg],
        &[],
        &pw,
    );
    assert!(r.param_copies.is_empty());
    assert_eq!(r.vreg_table[1], arg);
}

#[test]
fn copy_for_mutated_param() {
    let mut b = FunctionBuilder::new("c", &[IrType::I32]);
    let a = b.add_param(IrType::I32);
    let one = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: one, value: 1 });
    b.push(LpirOp::Iadd {
        dst: a,
        lhs: a,
        rhs: one,
    });
    b.push_return(&[a]);
    let callee = b.finish();
    let pw = scan_param_writes(&callee);
    let mut caller = FunctionBuilder::new("caller", &[IrType::I32]).finish();
    let arg = VReg(9);
    let r = build_remap(
        &mut caller,
        &callee,
        &[VMCTX_VREG, arg],
        &[],
        &pw,
    );
    assert_eq!(r.param_copies.len(), 1);
    match &r.param_copies[0] {
        LpirOp::Copy { dst, src } => {
            assert_eq!(*src, arg);
            assert_eq!(caller.vreg_types[dst.0 as usize], IrType::I32);
        }
        _ => panic!("expected Copy"),
    }
}

#[test]
fn vmctx_aliases() {
    let mut b = FunctionBuilder::new("c", &[IrType::I32]);
    let a = b.add_param(IrType::I32);
    b.push_return(&[a]);
    let callee = b.finish();
    let pw = scan_param_writes(&callee);
    let mut caller1 = FunctionBuilder::new("caller1", &[IrType::I32]).finish();
    let r = build_remap(
        &mut caller1,
        &callee,
        &[VMCTX_VREG, VReg(3)],
        &[],
        &pw,
    );
    assert_eq!(r.vreg_table[0], VMCTX_VREG);
    let mut caller2 = FunctionBuilder::new("caller2", &[IrType::I32]).finish();
    let r2 = build_remap(
        &mut caller2,
        &callee,
        &[VMCTX_VREG, VReg(3)],
        &[],
        &pw,
    );
    assert_eq!(r2.vreg_table[0], VMCTX_VREG);
}

#[test]
fn slot_offset_applied() {
    let callee = IrFunction {
        name: String::from("c"),
        is_entry: false,
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: vec![],
        vreg_types: vec![IrType::Pointer],
        slots: vec![
            SlotDecl { size: 4 },
            SlotDecl { size: 8 },
        ],
        body: vec![],
        vreg_pool: vec![],
    };
    let mut caller = IrFunction {
        name: String::from("x"),
        is_entry: false,
        vmctx_vreg: VMCTX_VREG,
        param_count: 0,
        return_types: vec![],
        vreg_types: vec![IrType::Pointer],
        slots: vec![
            SlotDecl { size: 1 },
            SlotDecl { size: 2 },
            SlotDecl { size: 3 },
        ],
        body: vec![],
        vreg_pool: vec![],
    };
    let pw = scan_param_writes(&callee);
    let r = build_remap(&mut caller, &callee, &[VMCTX_VREG], &[], &pw);
    assert_eq!(r.slot_offset, 3);
    assert_eq!(caller.slots.len(), 5);

    let mut pool = caller.vreg_pool.clone();
    let op = LpirOp::SlotAddr {
        dst: VReg(0),
        slot: crate::types::SlotId(0),
    };
    let out = remap_op(&op, &r, &mut pool, &callee.vreg_pool);
    match out {
        LpirOp::SlotAddr { slot, .. } => assert_eq!(slot.0, 3),
        _ => panic!("expected SlotAddr"),
    }
}

#[test]
fn vreg_pool_splice() {
    let mut mb = crate::builder::ModuleBuilder::new();
    let imp = mb.add_import(crate::lpir_module::ImportDecl {
        module_name: String::from("g"),
        func_name: String::from("sin"),
        param_types: vec![IrType::F32],
        return_types: vec![IrType::F32],
        lpfn_glsl_params: None,
        needs_vmctx: true,
    });
    let mut b = FunctionBuilder::new("c", &[IrType::F32]);
    let a = b.add_param(IrType::F32);
    b.push_call(imp, &[VMCTX_VREG, a], &[]);
    let r = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 { dst: r, value: 0.0 });
    b.push_return(&[r]);
    let callee = b.finish();
    let pw = scan_param_writes(&callee);
    let mut caller = FunctionBuilder::new("caller", &[IrType::F32]).finish();
    let arg = VReg(100);
    let remap = build_remap(
        &mut caller,
        &callee,
        &[VMCTX_VREG, arg],
        &[],
        &pw,
    );
    let call_op = callee
        .body
        .iter()
        .find(|o| matches!(o, LpirOp::Call { .. }))
        .expect("call")
        .clone();
    let mut pool = caller.vreg_pool.clone();
    let before_len = pool.len();
    let mapped = remap_op(&call_op, &remap, &mut pool, &callee.vreg_pool);
    assert!(pool.len() > before_len);
    match mapped {
        LpirOp::Call { args, .. } => {
            let slice = &pool[args.start as usize..args.start as usize + args.count as usize];
            assert_eq!(slice[0], VMCTX_VREG);
            assert_eq!(slice[1], arg);
        }
        _ => panic!("expected Call"),
    }
}
