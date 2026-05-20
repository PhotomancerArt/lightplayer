use alloc::format;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::arena::{ExprId, ExprList, HirArena};
use super::const_fold::fold_cast;
use super::scalar::{scalar_base_type, scalar_lane_count};
use super::types::HirExprKind;

pub(super) fn coerce_constructor_args(
    arena: &mut HirArena,
    span: Span,
    target_ty: &LpsType,
    args: ExprList,
) -> Result<ExprList, Diagnostic> {
    let arg_ids = arena.expr_list(args).to_vec();
    let expected_lanes = scalar_lane_count(target_ty);
    let actual_lanes = arg_ids
        .iter()
        .map(|arg| scalar_lane_count(arena.expr_ty(*arg)))
        .sum::<usize>();
    if target_ty.is_matrix() && arg_ids.len() == 1 && arena.expr_ty(arg_ids[0]).is_matrix() {
        return Ok(args);
    }
    if let LpsType::Array { element, len } = target_ty
        && arg_ids.len() == *len as usize
    {
        let mut coerced = Vec::with_capacity(arg_ids.len());
        for arg in arg_ids {
            coerced.push(coerce_expr(arena, arg, element)?);
        }
        return Ok(arena.push_expr_list(coerced));
    }
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
        let mut coerced = Vec::with_capacity(arg_ids.len());
        for arg in arg_ids {
            let arg_ty = arena.expr_ty(arg).clone();
            let arg_scalar = scalar_base_type(&arg_ty).unwrap_or_else(|| arg_ty.clone());
            if arg_scalar == expected_scalar {
                coerced.push(arg);
            } else {
                let target = if scalar_lane_count(&arg_ty) > 1 {
                    LpsType::vector_type(&expected_scalar, scalar_lane_count(&arg_ty))
                        .unwrap_or_else(|| expected_scalar.clone())
                } else {
                    expected_scalar.clone()
                };
                coerced.push(coerce_expr(arena, arg, &target)?);
            }
        }
        return Ok(arena.push_expr_list(coerced));
    }
    if arg_ids.len() == 1 && expected_lanes > 1 && scalar_lane_count(arena.expr_ty(arg_ids[0])) == 1
    {
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
    arena: &mut HirArena,
    span: Span,
    lhs: ExprId,
    rhs: ExprId,
) -> Result<(ExprId, ExprId, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[arena.expr_ty(lhs), arena.expr_ty(rhs)])
        .ok_or_else(|| Diagnostic::error(span, "unsupported arithmetic operand types"))?;
    if ty.is_matrix() {
        let lhs = coerce_matrix_arithmetic_operand(arena, lhs, &ty)?;
        let rhs = coerce_matrix_arithmetic_operand(arena, rhs, &ty)?;
        return Ok((lhs, rhs, ty));
    }
    Ok((
        coerce_expr(arena, lhs, &ty)?,
        coerce_expr(arena, rhs, &ty)?,
        ty,
    ))
}

fn coerce_matrix_arithmetic_operand(
    arena: &mut HirArena,
    expr: ExprId,
    matrix_ty: &LpsType,
) -> Result<ExprId, Diagnostic> {
    if arena.expr_ty(expr) == matrix_ty {
        return Ok(expr);
    }
    coerce_expr(arena, expr, &LpsType::Float)
}

pub(super) fn coerce_comparison_pair(
    arena: &mut HirArena,
    span: Span,
    lhs: ExprId,
    rhs: ExprId,
) -> Result<(ExprId, ExprId, LpsType), Diagnostic> {
    let ty = vector_dominant_type(&[arena.expr_ty(lhs), arena.expr_ty(rhs)])
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison operand types"))?;
    let result_ty = comparison_result_type(&ty)
        .ok_or_else(|| Diagnostic::error(span, "unsupported comparison result type"))?;
    Ok((
        coerce_expr(arena, lhs, &ty)?,
        coerce_expr(arena, rhs, &ty)?,
        result_ty,
    ))
}

pub(super) fn coerce_expr(
    arena: &mut HirArena,
    expr: ExprId,
    target: &LpsType,
) -> Result<ExprId, Diagnostic> {
    let expr_ty = arena.expr_ty(expr).clone();
    if expr_ty == *target {
        return Ok(expr);
    }
    if scalar_lane_count(&expr_ty) == 1 && scalar_lane_count(target) > 1 {
        let scalar = scalar_base_type(target).unwrap_or_else(|| target.clone());
        let expr = coerce_expr(arena, expr, &scalar)?;
        let args = arena.push_expr_list([expr]);
        let span = arena.expr_span(expr);
        return Ok(arena.push_expr(span, target.clone(), HirExprKind::Constructor { args }));
    }
    if scalar_lane_count(&expr_ty) == scalar_lane_count(target)
        && scalar_base_type(&expr_ty).is_some()
        && scalar_base_type(target).is_some()
    {
        return Ok(cast_expr(arena, expr, target.clone()));
    }
    match (&expr_ty, target) {
        (LpsType::Int, LpsType::Float)
        | (LpsType::UInt, LpsType::Float)
        | (LpsType::Float, LpsType::Int)
        | (LpsType::Float, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int)
        | (LpsType::Bool, LpsType::Float)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Float, LpsType::Bool)
        | (LpsType::Int, LpsType::Bool)
        | (LpsType::UInt, LpsType::Bool) => Ok(cast_expr(arena, expr, target.clone())),
        (LpsType::Bool, LpsType::Bool) => Ok(expr),
        _ => Err(Diagnostic::error(
            arena.expr_span(expr),
            format!("cannot coerce {expr_ty:?} to {target:?}"),
        )),
    }
}

fn cast_expr(arena: &mut HirArena, expr: ExprId, target: LpsType) -> ExprId {
    let span = arena.expr_span(expr);
    if let Some(folded) = fold_cast(span, &target, arena.expr(expr)) {
        return arena.push_expr(folded.span, folded.ty, folded.kind);
    }
    arena.push_expr(span, target, HirExprKind::Cast { expr })
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

pub(super) fn zero_expr(
    arena: &mut HirArena,
    span: Span,
    ty: &LpsType,
) -> Result<ExprId, Diagnostic> {
    if let LpsType::Array { element, len } = ty {
        let mut args = Vec::new();
        for _ in 0..*len {
            args.push(zero_expr(arena, span, element)?);
        }
        let args = arena.push_expr_list(args);
        return Ok(arena.push_expr(span, ty.clone(), HirExprKind::Constructor { args }));
    }
    if let LpsType::Struct { members, .. } = ty {
        let mut args = Vec::new();
        for member in members {
            args.push(zero_expr(arena, span, &member.ty)?);
        }
        let args = arena.push_expr_list(args);
        return Ok(arena.push_expr(span, ty.clone(), HirExprKind::Constructor { args }));
    }
    let scalar_ty = scalar_base_type(ty).unwrap_or_else(|| ty.clone());
    let kind = match scalar_ty {
        LpsType::Float => HirExprKind::FloatLiteral(0.0),
        LpsType::Int => HirExprKind::IntLiteral(0),
        LpsType::UInt => HirExprKind::UIntLiteral(0),
        LpsType::Bool => HirExprKind::BoolLiteral(false),
        _ => return Err(Diagnostic::error(span, "unsupported zero initializer type")),
    };
    let scalar = arena.push_expr(span, scalar_ty, kind);
    coerce_expr(arena, scalar, ty)
}
