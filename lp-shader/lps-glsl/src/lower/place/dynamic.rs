use lpir::{IrType, LpirOp, VReg};

use crate::{Diagnostic, Span};

use super::super::ops::single_lane;
use super::super::{LowerCtx, LowerValue};

pub(super) fn clamp_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    index: LowerValue,
    count: usize,
) -> Result<VReg, Diagnostic> {
    let index = single_lane(span, &index)?;
    Ok(clamp_index_vreg(ctx, index, count))
}

pub(super) fn clamp_index_vreg(ctx: &mut LowerCtx<'_>, index: VReg, count: usize) -> VReg {
    if count <= 1 {
        let zero = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: zero,
            value: 0,
        });
        return zero;
    }

    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });
    let below_zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS {
        dst: below_zero,
        lhs: index,
        rhs: zero,
    });
    let low_clamped = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: low_clamped,
        cond: below_zero,
        if_true: zero,
        if_false: index,
    });

    let last = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: last,
        value: count.saturating_sub(1) as i32,
    });
    let above_last = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgtS {
        dst: above_last,
        lhs: low_clamped,
        rhs: last,
    });
    let clamped = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: clamped,
        cond: above_last,
        if_true: last,
        if_false: low_clamped,
    });
    clamped
}

pub(super) fn scale_index(ctx: &mut LowerCtx<'_>, index: VReg, stride: usize) -> VReg {
    let byte_offset = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::ImulImm {
        dst: byte_offset,
        src: index,
        imm: stride as i32,
    });
    byte_offset
}

pub(super) fn add_offsets(ctx: &mut LowerCtx<'_>, lhs: Option<VReg>, rhs: VReg) -> VReg {
    let Some(lhs) = lhs else {
        return rhs;
    };
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd { dst, lhs, rhs });
    dst
}
