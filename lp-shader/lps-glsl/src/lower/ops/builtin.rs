use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::hir::{BuiltinKind, HirExpr, scalar_base_type, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue, lower_expr};
use super::numeric::{
    BinaryFloatOp, UnaryFloatOp, lane_at, lower_binary_float_lane, lower_bool_mix_lane,
    lower_min_max_lane, lower_mix_lane, lower_mod_lane, lower_smoothstep_lane,
    lower_unary_float_lane,
};
use super::scalar::lower_binary;

pub(in crate::lower) fn lower_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    args: &[HirExpr],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let values = args
        .iter()
        .map(|arg| lower_expr(ctx, arg))
        .collect::<Result<Vec<_>, _>>()?;
    if kind == BuiltinKind::Distance {
        let delta = lower_binary(
            ctx,
            span,
            BinaryOp::Sub,
            values[0].clone(),
            values[1].clone(),
            &values[0].ty,
        )?;
        return lower_length(ctx, span, &delta, result_ty);
    }
    if kind == BuiltinKind::Dot {
        return lower_dot(ctx, span, &values[0], &values[1], result_ty);
    }
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let lane = match kind {
            BuiltinKind::Abs => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Abs)?
            }
            BuiltinKind::Distance => {
                unreachable!("distance returns before lane-wise builtin lowering")
            }
            BuiltinKind::Dot => unreachable!("dot returns before lane-wise builtin lowering"),
            BuiltinKind::All | BuiltinKind::Any | BuiltinKind::Not => {
                return lower_bool_builtin(ctx, span, kind, &values[0], result_ty);
            }
            BuiltinKind::Equal => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Eq,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::GreaterThan => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Gt,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::GreaterThanEqual => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Ge,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::LessThan => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Lt,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::LessThanEqual => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Le,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::Length => return lower_length(ctx, span, &values[0], result_ty),
            BuiltinKind::NotEqual => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Ne,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::Floor => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Floor)?
            }
            BuiltinKind::Fract => {
                let x = lane_at(&values[0], i);
                let f = ctx.fb.alloc_vreg(IrType::F32);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Ffloor { dst: f, src: x });
                ctx.fb.push(LpirOp::Fsub {
                    dst,
                    lhs: x,
                    rhs: f,
                });
                dst
            }
            BuiltinKind::Min => {
                lower_min_max_lane(ctx, span, result_ty, &values[0], &values[1], i, true)?
            }
            BuiltinKind::Max => {
                lower_min_max_lane(ctx, span, result_ty, &values[0], &values[1], i, false)?
            }
            BuiltinKind::Mod => lower_mod_lane(ctx, &values[0], &values[1], i),
            BuiltinKind::Clamp => {
                let maxed =
                    lower_binary_float_lane(ctx, &values[0], &values[1], i, BinaryFloatOp::Max);
                let hi = lane_at(&values[2], i);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmin {
                    dst,
                    lhs: maxed,
                    rhs: hi,
                });
                dst
            }
            BuiltinKind::Mix if scalar_base_type(result_ty) == Some(LpsType::Bool) => {
                lower_bool_mix_lane(ctx, &values[0], &values[1], &values[2], i)
            }
            BuiltinKind::Mix => lower_mix_lane(ctx, &values[0], &values[1], &values[2], i),
            BuiltinKind::Smoothstep => {
                lower_smoothstep_lane(ctx, &values[0], &values[1], &values[2], i)
            }
            BuiltinKind::Sqrt => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fsqrt {
                    dst,
                    src: lane_at(&values[0], i),
                });
                dst
            }
        };
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_length(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if *result_ty != LpsType::Float || scalar_base_type(&value.ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(span, "length expects float lanes"));
    }
    let Some(first) = value.lanes.first().copied() else {
        return Err(Diagnostic::error(span, "length has no lanes"));
    };
    let mut sum = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: sum,
        lhs: first,
        rhs: first,
    });
    for lane in value.lanes.iter().skip(1) {
        let square = ctx.fb.alloc_vreg(IrType::F32);
        let next = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst: square,
            lhs: *lane,
            rhs: *lane,
        });
        ctx.fb.push(LpirOp::Fadd {
            dst: next,
            lhs: sum,
            rhs: square,
        });
        sum = next;
    }
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsqrt { dst, src: sum });
    Ok(LowerValue {
        ty: LpsType::Float,
        lanes: vec![dst],
    })
}

fn lower_dot(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: &LowerValue,
    rhs: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if *result_ty != LpsType::Float
        || scalar_base_type(&lhs.ty) != Some(LpsType::Float)
        || lhs.lanes.len() != rhs.lanes.len()
    {
        return Err(Diagnostic::error(span, "dot expects matching float lanes"));
    }
    let Some((&first_lhs, &first_rhs)) = lhs.lanes.first().zip(rhs.lanes.first()) else {
        return Err(Diagnostic::error(span, "dot has no lanes"));
    };
    let mut acc = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: acc,
        lhs: first_lhs,
        rhs: first_rhs,
    });
    for (l, r) in lhs.lanes.iter().zip(rhs.lanes.iter()).skip(1) {
        let product = ctx.fb.alloc_vreg(IrType::F32);
        let sum = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst: product,
            lhs: *l,
            rhs: *r,
        });
        ctx.fb.push(LpirOp::Fadd {
            dst: sum,
            lhs: acc,
            rhs: product,
        });
        acc = sum;
    }
    Ok(LowerValue {
        ty: LpsType::Float,
        lanes: vec![acc],
    })
}

pub(in crate::lower::ops) fn lower_bool_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    value: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if scalar_base_type(&value.ty) != Some(LpsType::Bool) {
        return Err(Diagnostic::error(span, "bool builtin expects bool lanes"));
    }
    match kind {
        BuiltinKind::All | BuiltinKind::Any => {
            let Some(mut acc) = value.lanes.first().copied() else {
                return Err(Diagnostic::error(span, "bool reduction has no lanes"));
            };
            for lane in value.lanes.iter().skip(1) {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                match kind {
                    BuiltinKind::All => ctx.fb.push(LpirOp::Iand {
                        dst,
                        lhs: acc,
                        rhs: *lane,
                    }),
                    BuiltinKind::Any => ctx.fb.push(LpirOp::Ior {
                        dst,
                        lhs: acc,
                        rhs: *lane,
                    }),
                    _ => unreachable!(),
                }
                acc = dst;
            }
            Ok(LowerValue {
                ty: result_ty.clone(),
                lanes: vec![acc],
            })
        }
        BuiltinKind::Not => {
            let mut lanes = Vec::new();
            for lane in &value.lanes {
                let zero = ctx.fb.alloc_vreg(IrType::I32);
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: zero,
                    value: 0,
                });
                ctx.fb.push(LpirOp::Ieq {
                    dst,
                    lhs: *lane,
                    rhs: zero,
                });
                lanes.push(dst);
            }
            Ok(LowerValue {
                ty: result_ty.clone(),
                lanes,
            })
        }
        _ => Err(Diagnostic::error(span, "unsupported bool builtin")),
    }
}
