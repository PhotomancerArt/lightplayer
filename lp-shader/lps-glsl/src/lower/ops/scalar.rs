use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::hir::{BuiltinKind, scalar_base_type, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
use super::builtin::lower_bool_builtin;
use super::matrix::lower_matrix_multiply;
use super::numeric::{lane_at, single_lane};

pub(in crate::lower) fn lower_binary(
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
