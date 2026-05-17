use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::hir::{BuiltinKind, HirUserCallWriteback, scalar_base_type, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
use super::numeric::lane_at;
use super::place_write::assign_target;

pub(in crate::lower) fn lower_integer_writeback_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    values: &[LowerValue],
    writebacks: &[HirUserCallWriteback],
    result_ty: &LpsType,
) -> Result<Option<LowerValue>, Diagnostic> {
    match kind {
        BuiltinKind::UaddCarry | BuiltinKind::UsubBorrow => {
            lower_add_sub_carry_builtin(ctx, span, kind, values, writebacks, result_ty).map(Some)
        }
        BuiltinKind::UmulExtended | BuiltinKind::ImulExtended => {
            lower_mul_extended_builtin(ctx, span, kind, values, writebacks, result_ty).map(Some)
        }
        _ => Ok(None),
    }
}

pub(in crate::lower) fn lower_bit_count_lane(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let x = lane_at(value, index);
    let one = iconst(ctx, 1);
    let mut acc = iconst(ctx, 0);
    for bit in 0..32 {
        let bit_value = lower_u32_bit_at(ctx, x, bit, one);
        let next = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Iadd {
            dst: next,
            lhs: acc,
            rhs: bit_value,
        });
        acc = next;
    }
    acc
}

pub(in crate::lower) fn lower_bitfield_reverse_lane(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let x = lane_at(value, index);
    let one = iconst(ctx, 1);
    let mut acc = iconst(ctx, 0);
    for bit in 0..32 {
        let bit_value = lower_u32_bit_at(ctx, x, bit, one);
        let reversed = ishl_imm(ctx, bit_value, 31 - bit);
        let next = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ior {
            dst: next,
            lhs: acc,
            rhs: reversed,
        });
        acc = next;
    }
    acc
}

pub(in crate::lower) fn lower_bitfield_extract_lane(
    ctx: &mut LowerCtx<'_>,
    result_ty: &LpsType,
    value: &LowerValue,
    offset: &LowerValue,
    bits: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let x = lane_at(value, index);
    let offset = lane_at(offset, index);
    let bits = lane_at(bits, index);
    let shifted = ctx.fb.alloc_vreg(IrType::I32);
    if scalar_base_type(result_ty) == Some(LpsType::Int) {
        ctx.fb.push(LpirOp::IshrS {
            dst: shifted,
            lhs: x,
            rhs: offset,
        });
    } else {
        ctx.fb.push(LpirOp::IshrU {
            dst: shifted,
            lhs: x,
            rhs: offset,
        });
    }
    let mask = lower_low_bits_mask(ctx, bits);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand {
        dst,
        lhs: shifted,
        rhs: mask,
    });
    dst
}

pub(in crate::lower) fn lower_bitfield_insert_lane(
    ctx: &mut LowerCtx<'_>,
    base: &LowerValue,
    insert: &LowerValue,
    offset: &LowerValue,
    bits: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let offset = lane_at(offset, index);
    let low_mask = lower_low_bits_mask(ctx, lane_at(bits, index));
    let mask = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ishl {
        dst: mask,
        lhs: low_mask,
        rhs: offset,
    });
    let inv_mask = ctx.fb.alloc_vreg(IrType::I32);
    let all_ones = iconst(ctx, -1);
    ctx.fb.push(LpirOp::Ixor {
        dst: inv_mask,
        lhs: mask,
        rhs: all_ones,
    });
    let cleared = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand {
        dst: cleared,
        lhs: lane_at(base, index),
        rhs: inv_mask,
    });
    let shifted_insert = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ishl {
        dst: shifted_insert,
        lhs: lane_at(insert, index),
        rhs: offset,
    });
    let masked_insert = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand {
        dst: masked_insert,
        lhs: shifted_insert,
        rhs: mask,
    });
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ior {
        dst,
        lhs: cleared,
        rhs: masked_insert,
    });
    dst
}

pub(in crate::lower) fn lower_find_lsb_lane(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let x = lane_at(value, index);
    let one = iconst(ctx, 1);
    let mut result = iconst(ctx, -1);
    for bit in (0..32).rev() {
        let bit_value = lower_u32_bit_at(ctx, x, bit, one);
        let present = ine_zero(ctx, bit_value);
        let bit_index = iconst(ctx, bit);
        let next = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Select {
            dst: next,
            cond: present,
            if_true: bit_index,
            if_false: result,
        });
        result = next;
    }
    result
}

pub(in crate::lower) fn lower_find_msb_lane(
    ctx: &mut LowerCtx<'_>,
    result_ty: &LpsType,
    value: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let mut x = lane_at(value, index);
    if scalar_base_type(result_ty) == Some(LpsType::Int) {
        let negative = ctx.fb.alloc_vreg(IrType::I32);
        let zero = iconst(ctx, 0);
        ctx.fb.push(LpirOp::IltS {
            dst: negative,
            lhs: x,
            rhs: zero,
        });
        let inverted = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ibnot {
            dst: inverted,
            src: x,
        });
        let selected = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Select {
            dst: selected,
            cond: negative,
            if_true: inverted,
            if_false: x,
        });
        x = selected;
    }

    let one = iconst(ctx, 1);
    let mut result = iconst(ctx, -1);
    for bit in 0..32 {
        let bit_value = lower_u32_bit_at(ctx, x, bit, one);
        let present = ine_zero(ctx, bit_value);
        let bit_index = iconst(ctx, bit);
        let next = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Select {
            dst: next,
            cond: present,
            if_true: bit_index,
            if_false: result,
        });
        result = next;
    }
    result
}

fn lower_add_sub_carry_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    values: &[LowerValue],
    writebacks: &[HirUserCallWriteback],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let [carry_writeback] = writebacks else {
        return Err(Diagnostic::error(span, "carry builtin writeback mismatch"));
    };
    let width = scalar_lane_count(result_ty);
    let mut result_lanes = Vec::new();
    let mut carry_lanes = Vec::new();
    for i in 0..width {
        let lhs = lane_at(&values[0], i);
        let rhs = lane_at(&values[1], i);
        let result = ctx.fb.alloc_vreg(IrType::I32);
        let carry = ctx.fb.alloc_vreg(IrType::I32);
        match kind {
            BuiltinKind::UaddCarry => {
                ctx.fb.push(LpirOp::Iadd {
                    dst: result,
                    lhs,
                    rhs,
                });
                ctx.fb.push(LpirOp::IltU {
                    dst: carry,
                    lhs: result,
                    rhs: lhs,
                });
            }
            BuiltinKind::UsubBorrow => {
                ctx.fb.push(LpirOp::Isub {
                    dst: result,
                    lhs,
                    rhs,
                });
                ctx.fb.push(LpirOp::IltU {
                    dst: carry,
                    lhs,
                    rhs,
                });
            }
            _ => unreachable!("not an add/sub carry builtin"),
        }
        result_lanes.push(result);
        carry_lanes.push(carry);
    }
    assign_target(
        ctx,
        span,
        &carry_writeback.target,
        LowerValue {
            ty: carry_writeback.ty.clone(),
            lanes: carry_lanes,
        },
    )?;
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes: result_lanes,
    })
}

fn lower_mul_extended_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    values: &[LowerValue],
    writebacks: &[HirUserCallWriteback],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let [msb_writeback, lsb_writeback] = writebacks else {
        return Err(Diagnostic::error(
            span,
            "multiply-extended builtin writeback mismatch",
        ));
    };
    let width = scalar_lane_count(&msb_writeback.ty);
    let mut msb_lanes = Vec::new();
    let mut lsb_lanes = Vec::new();
    for i in 0..width {
        let lhs = lane_at(&values[0], i);
        let rhs = lane_at(&values[1], i);
        let lsb = lower_mul_low_lane(ctx, lhs, rhs);
        let unsigned_msb = lower_umul_high_lane(ctx, lhs, rhs);
        let msb = match kind {
            BuiltinKind::UmulExtended => unsigned_msb,
            BuiltinKind::ImulExtended => lower_signed_mul_high_lane(ctx, lhs, rhs, unsigned_msb),
            _ => unreachable!("not a multiply-extended builtin"),
        };
        msb_lanes.push(msb);
        lsb_lanes.push(lsb);
    }
    assign_target(
        ctx,
        span,
        &msb_writeback.target,
        LowerValue {
            ty: msb_writeback.ty.clone(),
            lanes: msb_lanes,
        },
    )?;
    assign_target(
        ctx,
        span,
        &lsb_writeback.target,
        LowerValue {
            ty: lsb_writeback.ty.clone(),
            lanes: lsb_lanes,
        },
    )?;
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes: Vec::new(),
    })
}

fn lower_mul_low_lane(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Imul { dst, lhs, rhs });
    dst
}

fn lower_umul_high_lane(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let mask = iconst(ctx, 0xffff);
    let lhs_lo = iand(ctx, lhs, mask);
    let rhs_lo = iand(ctx, rhs, mask);
    let lhs_hi = ishr_u_imm(ctx, lhs, 16);
    let rhs_hi = ishr_u_imm(ctx, rhs, 16);

    let p0 = imul(ctx, lhs_lo, rhs_lo);
    let p1 = imul(ctx, lhs_lo, rhs_hi);
    let p2 = imul(ctx, lhs_hi, rhs_lo);
    let p3 = imul(ctx, lhs_hi, rhs_hi);

    let p0_hi = ishr_u_imm(ctx, p0, 16);
    let p1_lo = iand(ctx, p1, mask);
    let p2_lo = iand(ctx, p2, mask);
    let t0 = iadd(ctx, p0_hi, p1_lo);
    let t = iadd(ctx, t0, p2_lo);

    let p1_hi = ishr_u_imm(ctx, p1, 16);
    let p2_hi = ishr_u_imm(ctx, p2, 16);
    let t_hi = ishr_u_imm(ctx, t, 16);
    let hi0 = iadd(ctx, p3, p1_hi);
    let hi1 = iadd(ctx, hi0, p2_hi);
    iadd(ctx, hi1, t_hi)
}

fn lower_signed_mul_high_lane(
    ctx: &mut LowerCtx<'_>,
    lhs: lpir::VReg,
    rhs: lpir::VReg,
    unsigned_high: lpir::VReg,
) -> lpir::VReg {
    let zero = iconst(ctx, 0);
    let lhs_neg = ilt_s(ctx, lhs, zero);
    let rhs_neg = ilt_s(ctx, rhs, zero);
    let lhs_correction = select(ctx, lhs_neg, rhs, zero);
    let rhs_correction = select(ctx, rhs_neg, lhs, zero);
    let corrected_lhs = isub(ctx, unsigned_high, lhs_correction);
    isub(ctx, corrected_lhs, rhs_correction)
}

fn lower_low_bits_mask(ctx: &mut LowerCtx<'_>, bits: lpir::VReg) -> lpir::VReg {
    let one = iconst(ctx, 1);
    let shifted_one = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ishl {
        dst: shifted_one,
        lhs: one,
        rhs: bits,
    });
    let mask = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Isub {
        dst: mask,
        lhs: shifted_one,
        rhs: one,
    });
    mask
}

fn lower_u32_bit_at(
    ctx: &mut LowerCtx<'_>,
    value: lpir::VReg,
    bit: i32,
    one: lpir::VReg,
) -> lpir::VReg {
    let shifted = ishr_u_imm(ctx, value, bit);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand {
        dst,
        lhs: shifted,
        rhs: one,
    });
    dst
}

fn ishr_u_imm(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, imm: i32) -> lpir::VReg {
    let rhs = iconst(ctx, imm);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IshrU { dst, lhs, rhs });
    dst
}

fn iand(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iand { dst, lhs, rhs });
    dst
}

fn iadd(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd { dst, lhs, rhs });
    dst
}

fn isub(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Isub { dst, lhs, rhs });
    dst
}

fn imul(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Imul { dst, lhs, rhs });
    dst
}

fn ilt_s(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, rhs: lpir::VReg) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS { dst, lhs, rhs });
    dst
}

fn select(
    ctx: &mut LowerCtx<'_>,
    cond: lpir::VReg,
    if_true: lpir::VReg,
    if_false: lpir::VReg,
) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond,
        if_true,
        if_false,
    });
    dst
}

fn ishl_imm(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg, imm: i32) -> lpir::VReg {
    let rhs = iconst(ctx, imm);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ishl { dst, lhs, rhs });
    dst
}

fn ine_zero(ctx: &mut LowerCtx<'_>, lhs: lpir::VReg) -> lpir::VReg {
    let rhs = iconst(ctx, 0);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ine { dst, lhs, rhs });
    dst
}

fn iconst(ctx: &mut LowerCtx<'_>, value: i32) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 { dst, value });
    dst
}
