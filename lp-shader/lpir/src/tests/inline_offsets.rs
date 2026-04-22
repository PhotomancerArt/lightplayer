//! Tests for [`crate::inline::recompute_offsets`].

use crate::builder::FunctionBuilder;
use crate::inline::recompute_offsets;
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, ImportId, IrType};

fn zero_all_offsets(body: &mut [LpirOp]) {
    for op in body.iter_mut() {
        match op {
            LpirOp::IfStart {
                else_offset,
                end_offset,
                ..
            } => {
                *else_offset = 0;
                *end_offset = 0;
            }
            LpirOp::LoopStart {
                continuing_offset,
                end_offset,
            } => {
                *continuing_offset = 0;
                *end_offset = 0;
            }
            LpirOp::SwitchStart { end_offset, .. } => *end_offset = 0,
            LpirOp::CaseStart { end_offset, .. } | LpirOp::DefaultStart { end_offset } => {
                *end_offset = 0;
            }
            LpirOp::Block { end_offset } => *end_offset = 0,
            _ => {}
        }
    }
}

/// Collects all u32 offset fields from control ops in body order (for stable comparison).
fn flatten_control_offset_words(body: &[LpirOp]) -> alloc::vec::Vec<u32> {
    let mut w = alloc::vec::Vec::new();
    for op in body {
        match op {
            LpirOp::IfStart {
                else_offset,
                end_offset,
                ..
            } => {
                w.push(*else_offset);
                w.push(*end_offset);
            }
            LpirOp::LoopStart {
                continuing_offset,
                end_offset,
            } => {
                w.push(*continuing_offset);
                w.push(*end_offset);
            }
            LpirOp::SwitchStart { end_offset, .. } => w.push(*end_offset),
            LpirOp::CaseStart { end_offset, .. } | LpirOp::DefaultStart { end_offset } => {
                w.push(*end_offset);
            }
            LpirOp::Block { end_offset } => w.push(*end_offset),
            _ => {}
        }
    }
    w
}

fn assert_recompute_matches_original(mut original: alloc::vec::Vec<LpirOp>) {
    let expected = flatten_control_offset_words(&original);
    zero_all_offsets(&mut original);
    recompute_offsets(&mut original);
    assert_eq!(flatten_control_offset_words(&original), expected);
}

#[test]
fn if_else_end() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let _v = b.add_param(IrType::I32);
    let c = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: c, value: 1 });
    b.push_if(c);
    let t = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: t, value: 10 });
    b.push_else();
    let e = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: e, value: 20 });
    b.end_if();
    let f = b.finish();
    assert_recompute_matches_original(f.body);
}

#[test]
fn loop_with_continuing_marker() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let _v = b.add_param(IrType::I32);
    b.push_loop();
    let x = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: x, value: 0 });
    b.push_continuing();
    let y = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: y, value: 1 });
    b.end_loop();
    let f = b.finish();
    assert_recompute_matches_original(f.body);
}

#[test]
fn loop_without_continuing_marker() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let _v = b.add_param(IrType::I32);
    b.push_loop();
    let x = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: x, value: 0 });
    b.end_loop();
    let f = b.finish();
    let loop_pc = f
        .body
        .iter()
        .position(|op| matches!(op, LpirOp::LoopStart { .. }))
        .expect("LoopStart");
    let expected_co = (loop_pc + 1) as u32;
    let mut body = f.body;
    zero_all_offsets(&mut body);
    recompute_offsets(&mut body);
    if let LpirOp::LoopStart {
        continuing_offset, ..
    } = &body[loop_pc]
    {
        assert_eq!(*continuing_offset, expected_co);
    } else {
        panic!("expected LoopStart");
    }
}

#[test]
fn switch_multi_arm() {
    let mut b = FunctionBuilder::new("f", &[IrType::F32]);
    let sel = b.add_param(IrType::I32);
    b.push_switch(sel);
    b.push_case(0);
    let a = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 { dst: a, value: 1.0 });
    b.end_switch_arm();
    b.push_case(1);
    let c = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 { dst: c, value: 2.0 });
    b.end_switch_arm();
    b.push_default();
    let d = b.alloc_vreg(IrType::F32);
    b.push(LpirOp::FconstF32 {
        dst: d,
        value: -1.0,
    });
    b.end_switch_arm();
    b.end_switch();
    let f = b.finish();
    assert_recompute_matches_original(f.body);
}

#[test]
fn block_exit() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let _v = b.add_param(IrType::I32);
    b.push_block();
    b.push_exit_block();
    let x = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: x, value: 1 });
    b.end_block();
    let f = b.finish();
    assert_recompute_matches_original(f.body);
}

#[test]
fn nested_loop_in_if_in_block() {
    let mut b = FunctionBuilder::new("f", &[IrType::I32]);
    let p = b.add_param(IrType::I32);
    b.push_block();
    b.push_if(p);
    b.push_loop();
    let x = b.alloc_vreg(IrType::I32);
    b.push(LpirOp::IconstI32 { dst: x, value: 0 });
    b.end_loop();
    b.end_if();
    b.end_block();
    let f = b.finish();
    assert_recompute_matches_original(f.body);
}

#[test]
fn mutated_body_grows() {
    let mut b_ref = FunctionBuilder::new("f", &[IrType::I32]);
    let p = b_ref.add_param(IrType::I32);
    b_ref.push_if(p);
    let a = b_ref.alloc_vreg(IrType::I32);
    b_ref.push(LpirOp::IconstI32 { dst: a, value: 1 });
    let b_reg = b_ref.alloc_vreg(IrType::I32);
    b_ref.push(LpirOp::IconstI32 {
        dst: b_reg,
        value: 2,
    });
    b_ref.end_if();
    let reference = b_ref.finish();
    let expected_words = flatten_control_offset_words(&reference.body);

    let mut b_small = FunctionBuilder::new("f2", &[IrType::I32]);
    let p2 = b_small.add_param(IrType::I32);
    b_small.push_if(p2);
    let a2 = b_small.alloc_vreg(IrType::I32);
    b_small.push(LpirOp::IconstI32 { dst: a2, value: 1 });
    b_small.end_if();
    let mut grown = b_small.finish();
    // Grow to match reference: insert no-op call before closing `End`.
    let insert_at = grown.body.len() - 1;
    grown.body.insert(
        insert_at,
        LpirOp::Call {
            callee: CalleeRef::Import(ImportId(0)),
            args: crate::types::VRegRange::EMPTY,
            results: crate::types::VRegRange::EMPTY,
        },
    );
    zero_all_offsets(&mut grown.body);
    recompute_offsets(&mut grown.body);
    assert_eq!(
        flatten_control_offset_words(&grown.body),
        expected_words,
        "recomputed offsets should match a fresh build of the same control shape"
    );
}
