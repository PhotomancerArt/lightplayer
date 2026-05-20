use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::hir::{BuiltinKind, ExprId, HirUserCallWriteback, scalar_base_type, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue, lower_expr};
use super::builtin_integer::{
    lower_bit_count_lane, lower_bitfield_extract_lane, lower_bitfield_insert_lane,
    lower_bitfield_reverse_lane, lower_find_lsb_lane, lower_find_msb_lane,
    lower_integer_writeback_builtin,
};
use super::matrix::{lower_matrix_determinant, lower_matrix_inverse, lower_matrix_transpose};
use super::numeric::{
    BinaryFloatOp, UnaryFloatOp, fconst, lane_at, lower_binary_float_lane, lower_bool_mix_lane,
    lower_min_max_lane, lower_mix_lane, lower_mod_lane, lower_smoothstep_lane,
    lower_unary_float_lane,
};
use super::place_write::assign_target;
use super::scalar::lower_binary;

pub(in crate::lower) fn lower_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    args: &[ExprId],
    writebacks: &[HirUserCallWriteback],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let values = args
        .iter()
        .map(|arg| lower_expr(ctx, *arg))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(value) =
        lower_integer_writeback_builtin(ctx, span, kind, &values, writebacks, result_ty)?
    {
        return Ok(value);
    }
    if kind == BuiltinKind::Modf {
        return lower_modf_builtin(ctx, span, &values, writebacks, result_ty);
    }
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
    if kind == BuiltinKind::Cross {
        return lower_cross(ctx, span, &values[0], &values[1], result_ty);
    }
    if kind == BuiltinKind::Determinant {
        return lower_matrix_determinant(ctx, span, values[0].clone(), result_ty);
    }
    if kind == BuiltinKind::Inverse {
        return lower_matrix_inverse(ctx, span, values[0].clone(), result_ty);
    }
    if kind == BuiltinKind::Dot {
        return lower_dot(ctx, span, &values[0], &values[1], result_ty);
    }
    if kind == BuiltinKind::Normalize {
        return lower_normalize(ctx, span, &values[0], result_ty);
    }
    if kind == BuiltinKind::MatrixCompMult {
        return lower_matrix_comp_mult(ctx, span, &values[0], &values[1], result_ty);
    }
    if kind == BuiltinKind::OuterProduct {
        return lower_outer_product(ctx, span, &values[0], &values[1], result_ty);
    }
    if kind == BuiltinKind::Transpose {
        return lower_matrix_transpose(ctx, span, values[0].clone(), result_ty);
    }
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let lane = match kind {
            BuiltinKind::Abs => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Abs)?
            }
            BuiltinKind::Ceil => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Ceil)?
            }
            BuiltinKind::Degrees => {
                let scale = LowerValue {
                    ty: LpsType::Float,
                    lanes: vec![fconst(ctx, 180.0 / core::f32::consts::PI)],
                };
                lower_binary(
                    ctx,
                    span,
                    BinaryOp::Mul,
                    values[0].clone(),
                    scale,
                    result_ty,
                )?
                .lanes[i]
            }
            BuiltinKind::Cross => unreachable!("cross returns before lane-wise builtin lowering"),
            BuiltinKind::Determinant => {
                unreachable!("determinant returns before lane-wise builtin lowering")
            }
            BuiltinKind::Distance => {
                unreachable!("distance returns before lane-wise builtin lowering")
            }
            BuiltinKind::Dot => unreachable!("dot returns before lane-wise builtin lowering"),
            BuiltinKind::All | BuiltinKind::Any | BuiltinKind::Not => {
                return lower_bool_builtin(ctx, span, kind, &values[0], result_ty);
            }
            BuiltinKind::BitCount => lower_bit_count_lane(ctx, &values[0], i),
            BuiltinKind::BitfieldExtract => {
                lower_bitfield_extract_lane(ctx, result_ty, &values[0], &values[1], &values[2], i)
            }
            BuiltinKind::BitfieldInsert => {
                lower_bitfield_insert_lane(ctx, &values[0], &values[1], &values[2], &values[3], i)
            }
            BuiltinKind::BitfieldReverse => lower_bitfield_reverse_lane(ctx, &values[0], i),
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
            BuiltinKind::ImulExtended => {
                unreachable!("imulExtended returns before lane-wise builtin lowering")
            }
            BuiltinKind::Length => return lower_length(ctx, span, &values[0], result_ty),
            BuiltinKind::Inverse => {
                unreachable!("inverse returns before lane-wise builtin lowering")
            }
            BuiltinKind::InverseSqrt => lower_inversesqrt_lane(ctx, &values[0], i),
            BuiltinKind::IsInf | BuiltinKind::IsNan => iconst(ctx, 0),
            BuiltinKind::MatrixCompMult => {
                unreachable!("matrixCompMult returns before lane-wise builtin lowering")
            }
            BuiltinKind::Modf => unreachable!("modf returns before lane-wise builtin lowering"),
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
            BuiltinKind::Fma => lower_fma_lane(ctx, &values[0], &values[1], &values[2], i),
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
            BuiltinKind::FindLsb => lower_find_lsb_lane(ctx, &values[0], i),
            BuiltinKind::FindMsb => lower_find_msb_lane(ctx, result_ty, &values[0], i),
            BuiltinKind::Min => {
                lower_min_max_lane(ctx, span, result_ty, &values[0], &values[1], i, true)?
            }
            BuiltinKind::Max => {
                lower_min_max_lane(ctx, span, result_ty, &values[0], &values[1], i, false)?
            }
            BuiltinKind::Mod => lower_mod_lane(ctx, &values[0], &values[1], i),
            BuiltinKind::Normalize => {
                unreachable!("normalize returns before lane-wise builtin lowering")
            }
            BuiltinKind::OuterProduct => {
                unreachable!("outerProduct returns before lane-wise builtin lowering")
            }
            BuiltinKind::Radians => {
                let scale = LowerValue {
                    ty: LpsType::Float,
                    lanes: vec![fconst(ctx, core::f32::consts::PI / 180.0)],
                };
                lower_binary(
                    ctx,
                    span,
                    BinaryOp::Mul,
                    values[0].clone(),
                    scale,
                    result_ty,
                )?
                .lanes[i]
            }
            BuiltinKind::Round => lower_round_lane(ctx, &values[0], i),
            BuiltinKind::RoundEven => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fnearest {
                    dst,
                    src: lane_at(&values[0], i),
                });
                dst
            }
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
            BuiltinKind::Sign => lower_sign_lane(ctx, span, result_ty, &values[0], i)?,
            BuiltinKind::Sqrt => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fsqrt {
                    dst,
                    src: lane_at(&values[0], i),
                });
                dst
            }
            BuiltinKind::Transpose => {
                unreachable!("transpose returns before lane-wise builtin lowering")
            }
            BuiltinKind::Trunc => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Trunc)?
            }
            BuiltinKind::UaddCarry => {
                unreachable!("uaddCarry returns before lane-wise builtin lowering")
            }
            BuiltinKind::UmulExtended => {
                unreachable!("umulExtended returns before lane-wise builtin lowering")
            }
            BuiltinKind::UsubBorrow => {
                unreachable!("usubBorrow returns before lane-wise builtin lowering")
            }
        };
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_modf_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    values: &[LowerValue],
    writebacks: &[HirUserCallWriteback],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let [value] = values else {
        return Err(Diagnostic::error(
            span,
            "internal modf lowering expected a single argument",
        ));
    };
    let [integer_writeback] = writebacks else {
        return Err(Diagnostic::error(
            span,
            "internal modf lowering expected a single writeback",
        ));
    };
    let width = scalar_lane_count(result_ty);
    let mut integer_lanes = Vec::with_capacity(width);
    let mut fractional_lanes = Vec::with_capacity(width);
    for i in 0..width {
        let x = lane_at(value, i);
        let integer_lane = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Ftrunc {
            dst: integer_lane,
            src: x,
        });
        let fractional_lane = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fsub {
            dst: fractional_lane,
            lhs: x,
            rhs: integer_lane,
        });
        integer_lanes.push(integer_lane);
        fractional_lanes.push(fractional_lane);
    }
    assign_target(
        ctx,
        span,
        integer_writeback.target,
        LowerValue {
            ty: integer_writeback.ty.clone(),
            lanes: integer_lanes,
        },
    )?;
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes: fractional_lanes,
    })
}

fn lower_fma_lane(
    ctx: &mut LowerCtx<'_>,
    a: &LowerValue,
    b: &LowerValue,
    c: &LowerValue,
    index: usize,
) -> lpir::VReg {
    let product = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: product,
        lhs: lane_at(a, index),
        rhs: lane_at(b, index),
    });
    ctx.fb.push(LpirOp::Fadd {
        dst,
        lhs: product,
        rhs: lane_at(c, index),
    });
    dst
}

fn lower_inversesqrt_lane(ctx: &mut LowerCtx<'_>, value: &LowerValue, index: usize) -> lpir::VReg {
    let src = lane_at(value, index);
    let sqrt = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsqrt { dst: sqrt, src });
    let one = fconst(ctx, 1.0);
    let raw = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv {
        dst: raw,
        lhs: one,
        rhs: sqrt,
    });
    let zero = fconst(ctx, 0.0);
    let positive = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Fgt {
        dst: positive,
        lhs: src,
        rhs: zero,
    });
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: positive,
        if_true: raw,
        if_false: zero,
    });
    dst
}

fn lower_round_lane(ctx: &mut LowerCtx<'_>, value: &LowerValue, index: usize) -> lpir::VReg {
    let x = lane_at(value, index);
    let zero = fconst(ctx, 0.0);
    let half = fconst(ctx, 0.5);
    let positive = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Fge {
        dst: positive,
        lhs: x,
        rhs: zero,
    });

    let positive_shifted = ctx.fb.alloc_vreg(IrType::F32);
    let positive_rounded = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fadd {
        dst: positive_shifted,
        lhs: x,
        rhs: half,
    });
    ctx.fb.push(LpirOp::Ffloor {
        dst: positive_rounded,
        src: positive_shifted,
    });

    let negative_shifted = ctx.fb.alloc_vreg(IrType::F32);
    let negative_rounded = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: negative_shifted,
        lhs: x,
        rhs: half,
    });
    ctx.fb.push(LpirOp::Fceil {
        dst: negative_rounded,
        src: negative_shifted,
    });

    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: positive,
        if_true: positive_rounded,
        if_false: negative_rounded,
    });
    dst
}

fn lower_sign_lane(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    result_ty: &LpsType,
    value: &LowerValue,
    index: usize,
) -> Result<lpir::VReg, Diagnostic> {
    let x = lane_at(value, index);
    match scalar_base_type(result_ty).unwrap_or_else(|| result_ty.clone()) {
        LpsType::Float => Ok(lower_float_sign_lane(ctx, x)),
        LpsType::Int => Ok(lower_int_sign_lane(ctx, x)),
        LpsType::UInt => Ok(lower_uint_sign_lane(ctx, x)),
        _ => Err(Diagnostic::error(span, "sign expects numeric lanes")),
    }
}

fn lower_float_sign_lane(ctx: &mut LowerCtx<'_>, x: lpir::VReg) -> lpir::VReg {
    let zero = fconst(ctx, 0.0);
    let one = fconst(ctx, 1.0);
    let neg_one = fconst(ctx, -1.0);
    let gt = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Fgt {
        dst: gt,
        lhs: x,
        rhs: zero,
    });
    let lt = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Flt {
        dst: lt,
        lhs: x,
        rhs: zero,
    });
    let positive_or_zero = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst: positive_or_zero,
        cond: gt,
        if_true: one,
        if_false: zero,
    });
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: lt,
        if_true: neg_one,
        if_false: positive_or_zero,
    });
    dst
}

fn lower_int_sign_lane(ctx: &mut LowerCtx<'_>, x: lpir::VReg) -> lpir::VReg {
    let zero = iconst(ctx, 0);
    let gt = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgtS {
        dst: gt,
        lhs: x,
        rhs: zero,
    });
    let lt = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS {
        dst: lt,
        lhs: x,
        rhs: zero,
    });
    let one = iconst(ctx, 1);
    let neg_one = iconst(ctx, -1);
    let positive_or_zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: positive_or_zero,
        cond: gt,
        if_true: one,
        if_false: zero,
    });
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: lt,
        if_true: neg_one,
        if_false: positive_or_zero,
    });
    dst
}

fn lower_uint_sign_lane(ctx: &mut LowerCtx<'_>, x: lpir::VReg) -> lpir::VReg {
    let zero = iconst(ctx, 0);
    let gt = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgtU {
        dst: gt,
        lhs: x,
        rhs: zero,
    });
    let one = iconst(ctx, 1);
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst,
        cond: gt,
        if_true: one,
        if_false: zero,
    });
    dst
}

fn lower_matrix_comp_mult(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: &LowerValue,
    rhs: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if !result_ty.is_matrix() || lhs.ty != *result_ty || rhs.ty != *result_ty {
        return Err(Diagnostic::error(
            span,
            "matrixCompMult expects matching matrices",
        ));
    }
    if lhs.lanes.len() != rhs.lanes.len() {
        return Err(Diagnostic::error(span, "matrixCompMult lane counts differ"));
    }
    let mut lanes = Vec::new();
    for (l, r) in lhs.lanes.iter().zip(rhs.lanes.iter()) {
        let dst = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst,
            lhs: *l,
            rhs: *r,
        });
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_outer_product(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: &LowerValue,
    rhs: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = result_ty.matrix_dims() else {
        return Err(Diagnostic::error(
            span,
            "outerProduct result must be a matrix",
        ));
    };
    if lhs.lanes.len() != rows || rhs.lanes.len() != cols {
        return Err(Diagnostic::error(span, "unsupported outerProduct shape"));
    }
    let mut lanes = Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            let dst = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Fmul {
                dst,
                lhs: lhs.lanes[row],
                rhs: rhs.lanes[col],
            });
            lanes.push(dst);
        }
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn iconst(ctx: &mut LowerCtx<'_>, value: i32) -> lpir::VReg {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 { dst, value });
    dst
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

fn lower_cross(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: &LowerValue,
    rhs: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if *result_ty != LpsType::Vec3 || lhs.ty != LpsType::Vec3 || rhs.ty != LpsType::Vec3 {
        return Err(Diagnostic::error(span, "cross expects vec3 operands"));
    }
    let mut component = |a: usize, b: usize, c: usize, d: usize| {
        let left = ctx.fb.alloc_vreg(IrType::F32);
        let right = ctx.fb.alloc_vreg(IrType::F32);
        let dst = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Fmul {
            dst: left,
            lhs: lhs.lanes[a],
            rhs: rhs.lanes[b],
        });
        ctx.fb.push(LpirOp::Fmul {
            dst: right,
            lhs: lhs.lanes[c],
            rhs: rhs.lanes[d],
        });
        ctx.fb.push(LpirOp::Fsub {
            dst,
            lhs: left,
            rhs: right,
        });
        dst
    };
    Ok(LowerValue {
        ty: LpsType::Vec3,
        lanes: vec![
            component(1, 2, 2, 1),
            component(2, 0, 0, 2),
            component(0, 1, 1, 0),
        ],
    })
}

fn lower_normalize(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if scalar_base_type(result_ty) != Some(LpsType::Float) || value.ty != *result_ty {
        return Err(Diagnostic::error(span, "normalize expects float lanes"));
    }
    let length = lower_length(ctx, span, value, &LpsType::Float)?;
    lower_binary(ctx, span, BinaryOp::Div, value.clone(), length, result_ty)
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
