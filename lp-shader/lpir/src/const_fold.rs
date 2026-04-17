//! LPIR constant folding pass.
//!
//! Single forward pass over `IrFunction::body` that folds operations on
//! compile-time-known integer constants. Replaces in place without changing
//! the body length (control-flow offsets remain valid).

extern crate alloc;

use alloc::vec::Vec;

use crate::{IrFunction, LpirOp, VReg};

/// Fold constant integer operations in `func.body` in place.
///
/// Returns the number of operations folded.
pub fn fold_constants(func: &mut IrFunction) -> usize {
    let max_vreg = func.vreg_types.len();
    if max_vreg == 0 {
        return 0;
    }

    let mut vreg_val: Vec<Option<i32>> = Vec::new();
    vreg_val.resize(max_vreg, None);
    let mut folded = 0;

    for i in 0..func.body.len() {
        // Take ownership of the op temporarily to avoid borrow issues
        let op = core::mem::replace(
            &mut func.body[i],
            LpirOp::IconstI32 {
                dst: VReg(0),
                value: 0,
            },
        );

        let new_op = match &op {
            LpirOp::IconstI32 { dst, value } => {
                let idx = dst.0 as usize;
                if idx < vreg_val.len() {
                    vreg_val[idx] = Some(*value);
                }
                op
            }

            LpirOp::Iadd { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l.wrapping_add(r);
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Isub { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l.wrapping_sub(r);
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Imul { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l.wrapping_mul(r);
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Iand { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l & r;
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Ior { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l | r;
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Ixor { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l ^ r;
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Ishl { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l.wrapping_shl(r as u32);
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::IshrS { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = l.wrapping_shr(r as u32);
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::IshrU { dst, lhs, rhs } => {
                if let (Some(l), Some(r)) = (get(&vreg_val, *lhs), get(&vreg_val, *rhs)) {
                    let result = (l as u32).wrapping_shr(r as u32) as i32;
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Ineg { dst, src } => {
                if let Some(v) = get(&vreg_val, *src) {
                    let result = v.wrapping_neg();
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }
            LpirOp::Ibnot { dst, src } => {
                if let Some(v) = get(&vreg_val, *src) {
                    let result = !v;
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }

            LpirOp::IeqImm { dst, src, imm } => {
                if let Some(v) = get(&vreg_val, *src) {
                    let result = if v == *imm { 1 } else { 0 };
                    set(&mut vreg_val, *dst, result);
                    folded += 1;
                    LpirOp::IconstI32 {
                        dst: *dst,
                        value: result,
                    }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }

            LpirOp::Ieq { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l == r,
                &mut folded,
                op,
            ),
            LpirOp::Ine { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l != r,
                &mut folded,
                op,
            ),
            LpirOp::IltS { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l < r,
                &mut folded,
                op,
            ),
            LpirOp::IleS { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l <= r,
                &mut folded,
                op,
            ),
            LpirOp::IgtS { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l > r,
                &mut folded,
                op,
            ),
            LpirOp::IgeS { dst, lhs, rhs } => fold_icmp(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l >= r,
                &mut folded,
                op,
            ),
            LpirOp::IltU { dst, lhs, rhs } => fold_icmp_u(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l < r,
                &mut folded,
                op,
            ),
            LpirOp::IleU { dst, lhs, rhs } => fold_icmp_u(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l <= r,
                &mut folded,
                op,
            ),
            LpirOp::IgtU { dst, lhs, rhs } => fold_icmp_u(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l > r,
                &mut folded,
                op,
            ),
            LpirOp::IgeU { dst, lhs, rhs } => fold_icmp_u(
                &mut vreg_val,
                *dst,
                *lhs,
                *rhs,
                |l, r| l >= r,
                &mut folded,
                op,
            ),

            LpirOp::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => {
                if let Some(c) = get(&vreg_val, *cond) {
                    let src = if c != 0 { *if_true } else { *if_false };
                    if let Some(v) = get(&vreg_val, src) {
                        set(&mut vreg_val, *dst, v);
                    } else {
                        clear_vreg(&mut vreg_val, *dst);
                    }
                    folded += 1;
                    LpirOp::Copy { dst: *dst, src }
                } else {
                    clear_vreg(&mut vreg_val, *dst);
                    op
                }
            }

            LpirOp::IfStart { .. }
            | LpirOp::Else
            | LpirOp::End
            | LpirOp::LoopStart { .. }
            | LpirOp::Block { .. }
            | LpirOp::Break
            | LpirOp::Continue
            | LpirOp::BrIfNot { .. }
            | LpirOp::ExitBlock => {
                vreg_val.iter_mut().for_each(|v| *v = None);
                op
            }

            _ => {
                if let Some(dst) = op.def_vreg() {
                    clear_vreg(&mut vreg_val, dst);
                }
                op
            }
        };

        func.body[i] = new_op;
    }

    folded
}

fn get(vreg_val: &[Option<i32>], vreg: VReg) -> Option<i32> {
    vreg_val.get(vreg.0 as usize).copied().flatten()
}

fn set(vreg_val: &mut [Option<i32>], vreg: VReg, val: i32) {
    let idx = vreg.0 as usize;
    if idx < vreg_val.len() {
        vreg_val[idx] = Some(val);
    }
}

fn clear_vreg(vreg_val: &mut [Option<i32>], vreg: VReg) {
    let idx = vreg.0 as usize;
    if idx < vreg_val.len() {
        vreg_val[idx] = None;
    }
}

fn fold_icmp(
    vreg_val: &mut [Option<i32>],
    dst: VReg,
    lhs: VReg,
    rhs: VReg,
    cmp: fn(i32, i32) -> bool,
    folded: &mut usize,
    original_op: LpirOp,
) -> LpirOp {
    if let (Some(l), Some(r)) = (get(vreg_val, lhs), get(vreg_val, rhs)) {
        let result = if cmp(l, r) { 1 } else { 0 };
        set(vreg_val, dst, result);
        *folded += 1;
        LpirOp::IconstI32 { dst, value: result }
    } else {
        clear_vreg(vreg_val, dst);
        original_op
    }
}

fn fold_icmp_u(
    vreg_val: &mut [Option<i32>],
    dst: VReg,
    lhs: VReg,
    rhs: VReg,
    cmp: fn(u32, u32) -> bool,
    folded: &mut usize,
    original_op: LpirOp,
) -> LpirOp {
    if let (Some(l), Some(r)) = (get(vreg_val, lhs), get(vreg_val, rhs)) {
        let result = if cmp(l as u32, r as u32) { 1 } else { 0 };
        set(vreg_val, dst, result);
        *folded += 1;
        LpirOp::IconstI32 { dst, value: result }
    } else {
        clear_vreg(vreg_val, dst);
        original_op
    }
}
