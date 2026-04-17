//! Tests for [`crate::inline::remap::scan_param_writes`].

use alloc::vec;

use crate::builder::FunctionBuilder;
use crate::inline::remap::scan_param_writes;
use crate::lpir_op::LpirOp;
use crate::types::IrType;

#[test]
fn vmctx_never_written() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let _ = b.add_param(IrType::I32);
    let r = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: r, value: 1 });
    b.push_return(&[r]);
    let f = b.finish();
    let m = scan_param_writes(&f);
    assert!(
        m.written.is_empty() || !m.written.iter().any(|&x| x),
        "no params written"
    );
}

#[test]
fn single_param_read_only() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let a = b.add_param(IrType::I32);
    let r = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Iadd {
        dst: r,
        lhs: a,
        rhs: a,
    });
    b.push_return(&[r]);
    let f = b.finish();
    let m = scan_param_writes(&f);
    assert_eq!(m.written, vec![false]);
}

#[test]
fn single_param_mutated() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let a = b.add_param(IrType::I32);
    let one = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: one, value: 1 });
    b.push(LpirOp::Iadd {
        dst: a,
        lhs: a,
        rhs: one,
    });
    b.push_return(&[a]);
    let f = b.finish();
    let m = scan_param_writes(&f);
    assert_eq!(m.written, vec![true]);
}

#[test]
fn multi_param_mixed() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let p0 = b.add_param(IrType::I32);
    let p1 = b.add_param(IrType::I32);
    let _p2 = b.add_param(IrType::I32);
    b.push(LpirOp::Iadd {
        dst: p1,
        lhs: p1,
        rhs: p0,
    });
    let r = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::Iadd {
        dst: r,
        lhs: p0,
        rhs: p0,
    });
    b.push_return(&[r]);
    let f = b.finish();
    let m = scan_param_writes(&f);
    assert_eq!(m.written, vec![false, true, false]);
}
