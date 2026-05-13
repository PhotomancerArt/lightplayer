use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::scalar::{scalar_base_type, scalar_lane_count};
use super::types::{HirExpr, HirExprKind};

pub(super) fn coerce_constructor_args(
    span: Span,
    target_ty: &LpsType,
    args: Vec<HirExpr>,
) -> Result<Vec<HirExpr>, Diagnostic> {
    let expected_lanes = scalar_lane_count(target_ty);
    let actual_lanes = args
        .iter()
        .map(|arg| scalar_lane_count(&arg.ty))
        .sum::<usize>();
    if matches!(target_ty, LpsType::Struct { .. }) {
        if actual_lanes == expected_lanes {
            return Ok(args);
        }
        return Err(Diagnostic::error(
            span,
            format!(
                "constructor for {target_ty:?} expects {expected_lanes} scalar lanes, got {actual_lanes}"
            ),
        ));
    }
    if actual_lanes >= expected_lanes {
        let expected_scalar = scalar_base_type(target_ty).unwrap_or_else(|| target_ty.clone());
        return args
            .into_iter()
            .map(|arg| {
                let arg_scalar = scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone());
                if arg_scalar == expected_scalar {
                    Ok(arg)
                } else {
                    let target = if scalar_lane_count(&arg.ty) > 1 {
                        LpsType::vector_type(&expected_scalar, scalar_lane_count(&arg.ty))
                            .unwrap_or_else(|| expected_scalar.clone())
                    } else {
                        expected_scalar.clone()
                    };
                    coerce_expr(arg, &target)
                }
            })
            .collect();
    }
    if args.len() == 1 && expected_lanes > 1 && scalar_lane_count(&args[0].ty) == 1 {
        return Ok(args);
    }
    Err(Diagnostic::error(
        span,
        format!(
            "constructor for {target_ty:?} expects {expected_lanes} scalar lanes, got {actual_lanes}"
        ),
    ))
}

pub(super) fn coerce_arithmetic_pair(
    span: Span,
    lhs: HirExpr,
    rhs: HirExpr,
) -> Result<(HirExpr, HirExpr, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[&lhs.ty, &rhs.ty])
        .ok_or_else(|| Diagnostic::error(span, "unsupported arithmetic operand types"))?;
    Ok((coerce_expr(lhs, &ty)?, coerce_expr(rhs, &ty)?, ty))
}

pub(super) fn coerce_comparison_pair(
    span: Span,
    lhs: HirExpr,
    rhs: HirExpr,
) -> Result<(HirExpr, HirExpr, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[&lhs.ty, &rhs.ty])
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison operand types"))?;
    let result_ty = comparison_result_type(&ty)
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison result type"))?;
    Ok((coerce_expr(lhs, &ty)?, coerce_expr(rhs, &ty)?, result_ty))
}

pub(super) fn coerce_expr(expr: HirExpr, target: &LpsType) -> Result<HirExpr, Diagnostic> {
    if expr.ty == *target {
        return Ok(expr);
    }
    if scalar_lane_count(&expr.ty) == 1 && scalar_lane_count(target) > 1 {
        let scalar = scalar_base_type(target).unwrap_or_else(|| target.clone());
        let expr = coerce_expr(expr, &scalar)?;
        return Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Constructor {
                args: alloc::vec![expr],
            },
        });
    }
    if scalar_lane_count(&expr.ty) == scalar_lane_count(target)
        && scalar_base_type(&expr.ty).is_some()
        && scalar_base_type(target).is_some()
    {
        return Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        });
    }
    match (&expr.ty, target) {
        (LpsType::Int, LpsType::Float)
        | (LpsType::UInt, LpsType::Float)
        | (LpsType::Float, LpsType::Int)
        | (LpsType::Float, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int) => Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        }),
        (LpsType::Bool, LpsType::Float)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Float, LpsType::Bool)
        | (LpsType::Int, LpsType::Bool)
        | (LpsType::UInt, LpsType::Bool) => Ok(HirExpr {
            span: expr.span,
            ty: target.clone(),
            kind: HirExprKind::Cast {
                expr: Box::new(expr),
            },
        }),
        (LpsType::Bool, LpsType::Bool) => Ok(expr),
        _ => Err(Diagnostic::error(
            expr.span,
            format!("cannot coerce {:?} to {:?}", expr.ty, target),
        )),
    }
}

pub(super) fn vector_dominant_type(types: &[&LpsType]) -> Option<LpsType> {
    if let Some(matrix) = types.iter().find(|ty| ty.is_matrix()) {
        if types
            .iter()
            .all(|ty| **ty == **matrix || **ty == LpsType::Float)
        {
            return Some((*matrix).clone());
        }
        return None;
    }
    let mut lanes = 1usize;
    let mut base = LpsType::Bool;
    for ty in types {
        let ty_base = scalar_base_type(ty)?;
        if ty_base == LpsType::Float {
            base = LpsType::Float;
        } else if ty_base == LpsType::UInt && base != LpsType::Float {
            base = LpsType::UInt;
        } else if ty_base == LpsType::Int && base == LpsType::Bool {
            base = LpsType::Int;
        } else if ty_base != LpsType::Int && ty_base != LpsType::Bool {
            return None;
        }
        lanes = lanes.max(scalar_lane_count(ty));
    }
    if lanes == 1 {
        Some(base)
    } else {
        LpsType::vector_type(&base, lanes)
    }
}

pub(super) fn comparison_result_type(operand_ty: &LpsType) -> Option<LpsType> {
    match scalar_lane_count(operand_ty) {
        1 => Some(LpsType::Bool),
        lanes => LpsType::vector_type(&LpsType::Bool, lanes),
    }
}

pub(super) fn zero_expr(span: Span, ty: &LpsType) -> Result<HirExpr, Diagnostic> {
    if let LpsType::Struct { members, .. } = ty {
        let args = members
            .iter()
            .map(|member| zero_expr(span, &member.ty))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(HirExpr {
            span,
            ty: ty.clone(),
            kind: HirExprKind::Constructor { args },
        });
    }
    let scalar = match scalar_base_type(ty).unwrap_or_else(|| ty.clone()) {
        LpsType::Float => HirExpr {
            span,
            ty: LpsType::Float,
            kind: HirExprKind::FloatLiteral(0.0),
        },
        LpsType::Int => HirExpr {
            span,
            ty: LpsType::Int,
            kind: HirExprKind::IntLiteral(0),
        },
        LpsType::UInt => HirExpr {
            span,
            ty: LpsType::UInt,
            kind: HirExprKind::UIntLiteral(0),
        },
        LpsType::Bool => HirExpr {
            span,
            ty: LpsType::Bool,
            kind: HirExprKind::BoolLiteral(false),
        },
        _ => return Err(Diagnostic::error(span, "unsupported zero initializer type")),
    };
    coerce_expr(scalar, ty)
}
