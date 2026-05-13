use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, VReg};
use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::hir::{BuiltinKind, HirExpr, scalar_base_type, scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::{LowerCtx, LowerValue, lower_expr};

pub(super) fn lower_builtin(
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

fn lower_bool_builtin(
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

pub(super) fn lower_binary(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    op: BinaryOp,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if is_logical(op) {
        let lhs_lane = single_lane(span, &lhs)?;
        let rhs_lane = single_lane(span, &rhs)?;
        let dst = ctx.fb.alloc_vreg(IrType::I32);
        let op = match op {
            BinaryOp::LogicalAnd => LpirOp::Iand {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            BinaryOp::LogicalOr => LpirOp::Ior {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            BinaryOp::LogicalXor => LpirOp::Ixor {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            _ => unreachable!(),
        };
        ctx.fb.push(op);
        return Ok(LowerValue {
            ty: LpsType::Bool,
            lanes: vec![dst],
        });
    }
    if is_comparison(op) {
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && *result_ty == LpsType::Bool
            && lhs.lanes.len() > 1
        {
            let component_ty = LpsType::vector_type(&LpsType::Bool, lhs.lanes.len())
                .ok_or_else(|| Diagnostic::error(span, "unsupported aggregate comparison width"))?;
            let components = lower_binary(ctx, span, op, lhs, rhs, &component_ty)?;
            let reduction = if op == BinaryOp::Eq {
                BuiltinKind::All
            } else {
                BuiltinKind::Any
            };
            return lower_bool_builtin(ctx, span, reduction, &components, &LpsType::Bool);
        }
        let width = scalar_lane_count(result_ty);
        let mut lanes = Vec::new();
        for i in 0..width {
            let lhs_lane = lane_at(&lhs, i);
            let rhs_lane = lane_at(&rhs, i);
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            let base_ty = scalar_base_type(&lhs.ty).unwrap_or_else(|| lhs.ty.clone());
            let op = match base_ty {
                LpsType::Float => match op {
                    BinaryOp::Lt => LpirOp::Flt {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::Fle {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::Fgt {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::Fge {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Feq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Fne {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
                LpsType::UInt => match op {
                    BinaryOp::Lt => LpirOp::IltU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::IleU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::IgtU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::IgeU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Ieq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Ine {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
                _ => match op {
                    BinaryOp::Lt => LpirOp::IltS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::IleS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::IgtS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::IgeS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Ieq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Ine {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
            };
            ctx.fb.push(op);
            lanes.push(dst);
        }
        return Ok(LowerValue {
            ty: result_ty.clone(),
            lanes,
        });
    }
    if op == BinaryOp::Mul
        && lhs.ty.is_matrix()
        && rhs.ty.is_matrix()
        && lhs.ty == rhs.ty
        && *result_ty == lhs.ty
    {
        return lower_matrix_multiply(ctx, span, lhs, rhs, result_ty);
    }
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let l = lane_at(&lhs, i);
        let r = lane_at(&rhs, i);
        let base_ty = scalar_base_type(result_ty).unwrap_or_else(|| result_ty.clone());
        let dst = match base_ty {
            LpsType::Float => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                let op = match op {
                    BinaryOp::Add => LpirOp::Fadd {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Sub => LpirOp::Fsub {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mul => LpirOp::Fmul {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div => LpirOp::Fdiv {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    _ => return Err(Diagnostic::error(span, "unsupported float binary op")),
                };
                ctx.fb.push(op);
                dst
            }
            LpsType::Int | LpsType::UInt | LpsType::Bool => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                let op = match op {
                    BinaryOp::Add => LpirOp::Iadd {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Sub => LpirOp::Isub {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mul => LpirOp::Imul {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div if base_ty == LpsType::UInt => LpirOp::IdivU {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div => LpirOp::IdivS {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mod if base_ty == LpsType::UInt => LpirOp::IremU {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mod => LpirOp::IremS {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    _ => return Err(Diagnostic::error(span, "unsupported integer binary op")),
                };
                ctx.fb.push(op);
                dst
            }
            _ => return Err(Diagnostic::error(span, "unsupported binary result type")),
        };
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

pub(super) fn lower_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    target_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let src_base = scalar_base_type(&value.ty).ok_or_else(|| {
        Diagnostic::error(span, format!("unsupported cast source {:?}", value.ty))
    })?;
    let dst_base = scalar_base_type(target_ty)
        .ok_or_else(|| Diagnostic::error(span, format!("unsupported cast target {target_ty:?}")))?;
    if value.lanes.len() != scalar_lane_count(target_ty) {
        return Err(Diagnostic::error(span, "cast lane count mismatch"));
    }
    let dst_types = scalar_ir_types(target_ty)?;
    let mut lanes = Vec::new();
    for (src, dst_ty) in value.lanes.iter().zip(dst_types.iter()) {
        let dst = lower_scalar_cast(ctx, span, *src, &src_base, &dst_base, *dst_ty)?;
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: target_ty.clone(),
        lanes,
    })
}

fn lower_scalar_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    src: VReg,
    src_ty: &LpsType,
    dst_ty: &LpsType,
    dst_ir_ty: IrType,
) -> Result<VReg, Diagnostic> {
    let dst = ctx.fb.alloc_vreg(dst_ir_ty);
    match (src_ty, dst_ty) {
        (LpsType::Float, LpsType::Float)
        | (LpsType::Int, LpsType::Int)
        | (LpsType::UInt, LpsType::UInt)
        | (LpsType::Bool, LpsType::Bool)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int) => ctx.fb.push(LpirOp::Copy { dst, src }),
        (LpsType::Int, LpsType::Float) | (LpsType::Bool, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofS { dst, src });
        }
        (LpsType::UInt, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofU { dst, src });
        }
        (LpsType::Float, LpsType::Int) => {
            ctx.fb.push(LpirOp::FtoiSatS { dst, src });
        }
        (LpsType::Float, LpsType::UInt) => {
            ctx.fb.push(LpirOp::FtoiSatU { dst, src });
        }
        (LpsType::Int | LpsType::UInt, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: zero,
                value: 0,
            });
            ctx.fb.push(LpirOp::Ine {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        (LpsType::Float, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 {
                dst: zero,
                value: 0.0,
            });
            ctx.fb.push(LpirOp::Fne {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported scalar cast {src_ty:?} to {dst_ty:?}"),
            ));
        }
    }
    Ok(dst)
}

fn lower_matrix_multiply(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = result_ty.matrix_dims() else {
        return Err(Diagnostic::error(
            span,
            "matrix multiply result must be matrix",
        ));
    };
    if cols != rows || lhs.lanes.len() != cols * rows || rhs.lanes.len() != cols * rows {
        return Err(Diagnostic::error(span, "unsupported matrix multiply shape"));
    }
    let mut lanes = Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            let mut acc = None;
            for k in 0..cols {
                let product = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmul {
                    dst: product,
                    lhs: lhs.lanes[k * rows + row],
                    rhs: rhs.lanes[col * rows + k],
                });
                acc = Some(if let Some(prev) = acc {
                    let sum = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push(LpirOp::Fadd {
                        dst: sum,
                        lhs: prev,
                        rhs: product,
                    });
                    sum
                } else {
                    product
                });
            }
            lanes.push(acc.ok_or_else(|| Diagnostic::error(span, "empty matrix multiply"))?);
        }
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

mod access;

pub(super) use access::{assign_target, copy_value, lower_inc_dec, lower_index, lower_select};

#[derive(Debug, Clone, Copy)]
enum UnaryFloatOp {
    Abs,
    Floor,
}

fn lower_unary_float_lane(
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
        UnaryFloatOp::Floor => LpirOp::Ffloor { dst, src },
    });
    Ok(dst)
}

#[derive(Debug, Clone, Copy)]
enum BinaryFloatOp {
    Max,
}

fn lower_binary_float_lane(
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

fn lower_min_max_lane(
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

fn lower_mod_lane(
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
    dst
}

fn lower_mix_lane(
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

fn lower_bool_mix_lane(
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

fn lower_smoothstep_lane(
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

pub(super) fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 { dst, value });
    dst
}

pub(super) fn lane_at(value: &LowerValue, index: usize) -> VReg {
    value.lanes[index.min(value.lanes.len().saturating_sub(1))]
}

pub(super) fn single_lane(span: Span, value: &LowerValue) -> Result<VReg, Diagnostic> {
    match value.lanes.as_slice() {
        [lane] => Ok(*lane),
        _ => Err(Diagnostic::error(span, "expected scalar value")),
    }
}

fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

fn is_logical(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::LogicalXor
    )
}
