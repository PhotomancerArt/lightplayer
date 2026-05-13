use alloc::format;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::coerce::{
    coerce_arithmetic_pair, coerce_comparison_pair, coerce_expr, vector_dominant_type,
};
use super::scalar::{scalar_base_type, scalar_lane_count};
use super::types::{BuiltinKind, HirExpr};

pub(super) fn builtin_kind(name: &str) -> Option<BuiltinKind> {
    Some(match name {
        "abs" => BuiltinKind::Abs,
        "all" => BuiltinKind::All,
        "any" => BuiltinKind::Any,
        "ceil" => BuiltinKind::Ceil,
        "clamp" => BuiltinKind::Clamp,
        "degrees" => BuiltinKind::Degrees,
        "distance" => BuiltinKind::Distance,
        "dot" => BuiltinKind::Dot,
        "equal" => BuiltinKind::Equal,
        "floor" => BuiltinKind::Floor,
        "fract" => BuiltinKind::Fract,
        "greaterThan" => BuiltinKind::GreaterThan,
        "greaterThanEqual" => BuiltinKind::GreaterThanEqual,
        "length" => BuiltinKind::Length,
        "lessThan" => BuiltinKind::LessThan,
        "lessThanEqual" => BuiltinKind::LessThanEqual,
        "max" => BuiltinKind::Max,
        "min" => BuiltinKind::Min,
        "mix" => BuiltinKind::Mix,
        "mod" => BuiltinKind::Mod,
        "not" => BuiltinKind::Not,
        "normalize" => BuiltinKind::Normalize,
        "notEqual" => BuiltinKind::NotEqual,
        "radians" => BuiltinKind::Radians,
        "round" => BuiltinKind::Round,
        "smoothstep" => BuiltinKind::Smoothstep,
        "sqrt" => BuiltinKind::Sqrt,
        "trunc" => BuiltinKind::Trunc,
        _ => return None,
    })
}

pub(super) fn is_glsl_import(name: &str) -> bool {
    matches!(
        name,
        "sin" | "cos" | "asin" | "acos" | "exp" | "exp2" | "log" | "log2" | "pow" | "atan"
    )
}

pub(super) fn type_glsl_import_args(
    span: Span,
    name: &str,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    if matches!(
        name,
        "sin" | "cos" | "asin" | "acos" | "exp" | "exp2" | "log" | "log2"
    ) && args.len() == 1
    {
        let arg = args[0].clone();
        let arg_base = scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone());
        if arg_base == LpsType::Float {
            return Ok((args, arg.ty));
        }
        let arg = coerce_expr(arg, &LpsType::Float)?;
        return Ok((alloc::vec![arg], LpsType::Float));
    }
    if name == "pow" && args.len() == 2 {
        let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
        if scalar_base_type(&ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "pow expects float lanes"));
        }
        return Ok((alloc::vec![a, b], ty));
    }

    let args = args
        .into_iter()
        .map(|arg| coerce_expr(arg, &LpsType::Float))
        .collect::<Result<Vec<_>, _>>()?;
    let ty = match name {
        "atan" if args.len() == 1 || args.len() == 2 => LpsType::Float,
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported GLSL import signature `{name}`"),
            ));
        }
    };
    Ok((args, ty))
}

pub(super) fn type_builtin_args(
    span: Span,
    kind: BuiltinKind,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    let arity = match kind {
        BuiltinKind::Abs
        | BuiltinKind::All
        | BuiltinKind::Any
        | BuiltinKind::Ceil
        | BuiltinKind::Degrees
        | BuiltinKind::Floor
        | BuiltinKind::Fract
        | BuiltinKind::Length
        | BuiltinKind::Normalize
        | BuiltinKind::Not
        | BuiltinKind::Radians
        | BuiltinKind::Round
        | BuiltinKind::Sqrt
        | BuiltinKind::Trunc => 1,
        BuiltinKind::Equal
        | BuiltinKind::Distance
        | BuiltinKind::Dot
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::Max
        | BuiltinKind::Min
        | BuiltinKind::Mod
        | BuiltinKind::NotEqual => 2,
        BuiltinKind::Clamp | BuiltinKind::Mix | BuiltinKind::Smoothstep => 3,
    };
    if args.len() != arity {
        return Err(Diagnostic::error(
            span,
            format!("builtin expects {arity} arguments"),
        ));
    }
    match kind {
        BuiltinKind::Abs | BuiltinKind::Floor | BuiltinKind::Fract => {
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Ceil
        | BuiltinKind::Degrees
        | BuiltinKind::Radians
        | BuiltinKind::Round
        | BuiltinKind::Sqrt
        | BuiltinKind::Trunc => {
            if scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "builtin expects float lanes"));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Length | BuiltinKind::Normalize => {
            if scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "builtin expects float lanes"));
            }
            let ty = match kind {
                BuiltinKind::Length => LpsType::Float,
                BuiltinKind::Normalize => args[0].ty.clone(),
                _ => unreachable!(),
            };
            Ok((args, ty))
        }
        BuiltinKind::Distance | BuiltinKind::Dot => {
            let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            if scalar_base_type(&ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "builtin expects float lanes"));
            }
            Ok((alloc::vec![a, b], LpsType::Float))
        }
        BuiltinKind::All | BuiltinKind::Any => {
            let arg = coerce_expr(args[0].clone(), &args[0].ty)?;
            if scalar_base_type(&arg.ty) != Some(LpsType::Bool) {
                return Err(Diagnostic::error(span, "all/any expects bool lanes"));
            }
            Ok((alloc::vec![arg], LpsType::Bool))
        }
        BuiltinKind::Not => {
            let arg = args[0].clone();
            let ty = arg.ty.clone();
            if scalar_base_type(&ty) != Some(LpsType::Bool) {
                return Err(Diagnostic::error(span, "not expects bool lanes"));
            }
            Ok((alloc::vec![arg], ty))
        }
        BuiltinKind::Max | BuiltinKind::Min | BuiltinKind::Mod => {
            let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            Ok((alloc::vec![a, b], ty))
        }
        BuiltinKind::Equal
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::NotEqual => {
            let (a, b, ty) = coerce_comparison_pair(span, args[0].clone(), args[1].clone())?;
            Ok((alloc::vec![a, b], ty))
        }
        BuiltinKind::Clamp | BuiltinKind::Smoothstep => {
            let (a, b, ty_ab) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            let c = coerce_expr(args[2].clone(), &ty_ab).or_else(|_| {
                let (_, c, _) = coerce_arithmetic_pair(span, a.clone(), args[2].clone())?;
                Ok::<_, Diagnostic>(c)
            })?;
            let ty = vector_dominant_type(&[&a.ty, &b.ty, &c.ty])
                .ok_or_else(|| Diagnostic::error(span, "unsupported builtin argument types"))?;
            Ok((
                alloc::vec![
                    coerce_expr(a, &ty)?,
                    coerce_expr(b, &ty)?,
                    coerce_expr(c, &ty)?
                ],
                ty,
            ))
        }
        BuiltinKind::Mix => {
            let (x, y, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            let a = if scalar_lane_count(&args[2].ty) == 1 {
                coerce_expr(args[2].clone(), &LpsType::Float)?
            } else {
                coerce_expr(args[2].clone(), &ty)?
            };
            Ok((alloc::vec![x, y, a], ty))
        }
    }
}
