use lpir::{IrType, LpirOp, VReg};
use lps_shared::LpsType;

use crate::hir::scalar_base_type;
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};

#[derive(Debug, Clone, Copy)]
pub(in crate::lower::ops) enum UnaryFloatOp {
    Abs,
    Ceil,
    Floor,
    Trunc,
}

pub(in crate::lower::ops) fn lower_unary_float_lane(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    result_ty: &LpsType,
    value: &LowerValue,
    index: usize,
    op: UnaryFloatOp,
) -> Result<VReg, Diagnostic> {
    if scalar_base_type(result_ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(
            span,
            "builtin currently expects float lanes",
        ));
    }
    let src = lane_at(value, index);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(match op {
        UnaryFloatOp::Abs => LpirOp::Fabs { dst, src },
        UnaryFloatOp::Ceil => LpirOp::Fceil { dst, src },
        UnaryFloatOp::Floor => LpirOp::Ffloor { dst, src },
        UnaryFloatOp::Trunc => LpirOp::Ftrunc { dst, src },
    });
    Ok(dst)
}

#[derive(Debug, Clone, Copy)]
pub(in crate::lower::ops) enum BinaryFloatOp {
    Max,
}

pub(in crate::lower::ops) fn lower_binary_float_lane(
    ctx: &mut LowerCtx<'_>,
    lhs: &LowerValue,
    rhs: &LowerValue,
    index: usize,
    op: BinaryFloatOp,
) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    let lhs = lane_at(lhs, index);
    let rhs = lane_at(rhs, index);
    ctx.fb.push(match op {
        BinaryFloatOp::Max => LpirOp::Fmax { dst, lhs, rhs },
    });
    dst
}

pub(in crate::lower::ops) fn lower_min_max_lane(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    result_ty: &LpsType,
    lhs: &LowerValue,
    rhs: &LowerValue,
    index: usize,
    is_min: bool,
) -> Result<VReg, Diagnostic> {
    let lhs = lane_at(lhs, index);
    let rhs = lane_at(rhs, index);
    match scalar_base_type(result_ty).unwrap_or_else(|| result_ty.clone()) {
        LpsType::Float => {
            let dst = ctx.fb.alloc_vreg(IrType::F32);
            if is_min {
                ctx.fb.push(LpirOp::Fmin { dst, lhs, rhs });
            } else {
                ctx.fb.push(LpirOp::Fmax { dst, lhs, rhs });
            }
            Ok(dst)
        }
        LpsType::Int | LpsType::UInt => {
            let cond = ctx.fb.alloc_vreg(IrType::I32);
            let is_uint = scalar_base_type(result_ty) == Some(LpsType::UInt);
            match (is_min, is_uint) {
                (true, true) => ctx.fb.push(LpirOp::IltU {
                    dst: cond,
                    lhs,
                    rhs,
                }),
                (true, false) => ctx.fb.push(LpirOp::IltS {
                    dst: cond,
                    lhs,
                    rhs,
                }),
                (false, true) => ctx.fb.push(LpirOp::IgtU {
                    dst: cond,
                    lhs,
                    rhs,
                }),
                (false, false) => ctx.fb.push(LpirOp::IgtS {
                    dst: cond,
                    lhs,
                    rhs,
                }),
            }
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Select {
                dst,
                cond,
                if_true: lhs,
                if_false: rhs,
            });
            Ok(dst)
        }
        _ => Err(Diagnostic::error(span, "min/max expects numeric lanes")),
    }
}

pub(in crate::lower::ops) fn lower_mod_lane(
    ctx: &mut LowerCtx<'_>,
    lhs: &LowerValue,
    rhs: &LowerValue,
    index: usize,
) -> VReg {
    let lhs = lane_at(lhs, index);
    let rhs = lane_at(rhs, index);
    let div = ctx.fb.alloc_vreg(IrType::F32);
    let floored = ctx.fb.alloc_vreg(IrType::F32);
    let scaled = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv { dst: div, lhs, rhs });
    ctx.fb.push(LpirOp::Ffloor {
        dst: floored,
        src: div,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: scaled,
        lhs: rhs,
        rhs: floored,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst,
        lhs,
        rhs: scaled,
    });
    let delta = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: delta,
        lhs: dst,
        rhs,
    });
    let distance = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fabs {
        dst: distance,
        src: delta,
    });
    let eps = fconst(ctx, 0.001);
    let close_to_divisor = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Flt {
        dst: close_to_divisor,
        lhs: distance,
        rhs: eps,
    });
    let zero = fconst(ctx, 0.0);
    let corrected = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst: corrected,
        cond: close_to_divisor,
        if_true: zero,
        if_false: dst,
    });
    corrected
}

pub(in crate::lower::ops) fn lower_mix_lane(
    ctx: &mut LowerCtx<'_>,
    x: &LowerValue,
    y: &LowerValue,
    a: &LowerValue,
    index: usize,
) -> VReg {
    let x = lane_at(x, index);
    let y = lane_at(y, index);
    let a = lane_at(a, index);
    let one = fconst(ctx, 1.0);
    let inv = ctx.fb.alloc_vreg(IrType::F32);
    let left = ctx.fb.alloc_vreg(IrType::F32);
    let right = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: inv,
        lhs: one,
        rhs: a,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: left,
        lhs: x,
        rhs: inv,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: right,
        lhs: y,
        rhs: a,
    });
    ctx.fb.push(LpirOp::Fadd {
        dst,
        lhs: left,
        rhs: right,
    });
    dst
}

pub(in crate::lower::ops) fn lower_bool_mix_lane(
    ctx: &mut LowerCtx<'_>,
    x: &LowerValue,
    y: &LowerValue,
    selector: &LowerValue,
    index: usize,
) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: lane_at(selector, index),
        if_true: lane_at(y, index),
        if_false: lane_at(x, index),
    });
    dst
}

pub(in crate::lower::ops) fn lower_smoothstep_lane(
    ctx: &mut LowerCtx<'_>,
    edge0: &LowerValue,
    edge1: &LowerValue,
    x: &LowerValue,
    index: usize,
) -> VReg {
    let e0 = lane_at(edge0, index);
    let e1 = lane_at(edge1, index);
    let x = lane_at(x, index);
    let num = ctx.fb.alloc_vreg(IrType::F32);
    let den = ctx.fb.alloc_vreg(IrType::F32);
    let raw_t = ctx.fb.alloc_vreg(IrType::F32);
    let zero = fconst(ctx, 0.0);
    let one = fconst(ctx, 1.0);
    ctx.fb.push(LpirOp::Fsub {
        dst: num,
        lhs: x,
        rhs: e0,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst: den,
        lhs: e1,
        rhs: e0,
    });
    ctx.fb.push(LpirOp::Fdiv {
        dst: raw_t,
        lhs: num,
        rhs: den,
    });
    let low = ctx.fb.alloc_vreg(IrType::F32);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmax {
        dst: low,
        lhs: raw_t,
        rhs: zero,
    });
    ctx.fb.push(LpirOp::Fmin {
        dst: t,
        lhs: low,
        rhs: one,
    });
    let two = fconst(ctx, 2.0);
    let three = fconst(ctx, 3.0);
    let tt = ctx.fb.alloc_vreg(IrType::F32);
    let two_t = ctx.fb.alloc_vreg(IrType::F32);
    let factor = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: tt,
        lhs: t,
        rhs: t,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: two_t,
        lhs: two,
        rhs: t,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst: factor,
        lhs: three,
        rhs: two_t,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst,
        lhs: tt,
        rhs: factor,
    });
    dst
}

pub(in crate::lower::ops) fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 { dst, value });
    dst
}

pub(in crate::lower) fn lane_at(value: &LowerValue, index: usize) -> VReg {
    value.lanes[index.min(value.lanes.len().saturating_sub(1))]
}

pub(in crate::lower) fn single_lane(span: Span, value: &LowerValue) -> Result<VReg, Diagnostic> {
    match value.lanes.as_slice() {
        [lane] => Ok(*lane),
        _ => Err(Diagnostic::error(span, "expected scalar value")),
    }
}
