use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::body::BinaryOp;
use crate::{Diagnostic, Span};

use super::types::{BuiltinKind, HirExpr, HirExprKind};

pub(super) fn builtin_kind(name: &str) -> Option<BuiltinKind> {
    Some(match name {
        "abs" => BuiltinKind::Abs,
        "all" => BuiltinKind::All,
        "any" => BuiltinKind::Any,
        "clamp" => BuiltinKind::Clamp,
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
        "notEqual" => BuiltinKind::NotEqual,
        "smoothstep" => BuiltinKind::Smoothstep,
        "sqrt" => BuiltinKind::Sqrt,
        _ => return None,
    })
}

pub(super) fn is_glsl_import(name: &str) -> bool {
    matches!(name, "sin" | "cos" | "exp" | "atan")
}

pub(super) fn type_glsl_import_args(
    span: Span,
    name: &str,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    if matches!(name, "sin" | "cos" | "exp" | "sqrt") && args.len() == 1 {
        let arg = args[0].clone();
        let arg_base = scalar_base_type(&arg.ty).unwrap_or_else(|| arg.ty.clone());
        if arg_base == LpsType::Float {
            return Ok((args, arg.ty));
        }
        let arg = coerce_expr(arg, &LpsType::Float)?;
        return Ok((alloc::vec![arg], LpsType::Float));
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
        | BuiltinKind::Floor
        | BuiltinKind::Fract
        | BuiltinKind::Length
        | BuiltinKind::Not
        | BuiltinKind::Sqrt => 1,
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
        BuiltinKind::Sqrt => {
            if scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "sqrt expects float lanes"));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Length => {
            if scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "length expects float lanes"));
            }
            Ok((args, LpsType::Float))
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
                "constructor for {:?} expects {expected_lanes} scalar lanes, got {actual_lanes}",
                target_ty
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
            "constructor for {:?} expects {expected_lanes} scalar lanes, got {actual_lanes}",
            target_ty
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

pub(super) fn access_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    if let Some((offset, field_ty)) = struct_field_lanes(ty, fields) {
        let width = scalar_lane_count(&field_ty);
        return Ok(((offset..offset + width).collect(), field_ty));
    }
    swizzle_lanes(span, ty, fields)
}

fn struct_field_lanes(ty: &LpsType, field: &str) -> Option<(usize, LpsType)> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    let mut offset = 0usize;
    for member in members {
        if member.name.as_deref() == Some(field) {
            return Some((offset, member.ty.clone()));
        }
        offset = offset.saturating_add(scalar_lane_count(&member.ty));
    }
    None
}

fn swizzle_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    let count = scalar_lane_count(ty);
    if count < 2 {
        return Err(Diagnostic::error(span, "swizzle requires vector base"));
    }
    let mut lanes = Vec::new();
    for ch in fields.chars() {
        let lane = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => return Err(Diagnostic::error(span, "unsupported swizzle field")),
        };
        if lane >= count {
            return Err(Diagnostic::error(span, "swizzle lane out of range"));
        }
        lanes.push(lane);
    }
    let base = scalar_base_type(ty).ok_or_else(|| Diagnostic::error(span, "swizzle base type"))?;
    let out_ty = if lanes.len() == 1 {
        base
    } else {
        LpsType::vector_type(&base, lanes.len())
            .ok_or_else(|| Diagnostic::error(span, "unsupported swizzle width"))?
    };
    Ok((lanes, out_ty))
}

pub(super) fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

pub(super) fn is_logical(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::LogicalXor
    )
}

pub(super) fn glsl_param_token(ty: &LpsType, span: Span) -> Result<String, Diagnostic> {
    Ok(match ty {
        LpsType::Float => String::from("Float"),
        LpsType::Int => String::from("Int"),
        LpsType::UInt => String::from("UInt"),
        LpsType::Vec2 => String::from("Vec2"),
        LpsType::Vec3 => String::from("Vec3"),
        LpsType::Vec4 => String::from("Vec4"),
        other => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported LPFN parameter type {other:?}"),
            ));
        }
    })
}

pub fn scalar_lane_count(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Array { element, len } => scalar_lane_count(element).saturating_mul(*len as usize),
        LpsType::Struct { members, .. } => members
            .iter()
            .map(|member| scalar_lane_count(&member.ty))
            .sum(),
        _ => ty
            .component_count()
            .or_else(|| ty.matrix_element_count())
            .unwrap_or(0),
    }
}

pub fn scalar_base_type(ty: &LpsType) -> Option<LpsType> {
    if let LpsType::Array { element, .. } = ty {
        scalar_base_type(element)
    } else if ty.is_matrix() {
        Some(LpsType::Float)
    } else if ty.is_vector() {
        ty.vector_base_type()
    } else if ty.is_scalar() {
        Some(ty.clone())
    } else {
        None
    }
}

pub fn scalar_ir_types(ty: &LpsType) -> Result<Vec<lpir::IrType>, Diagnostic> {
    if *ty == LpsType::Void {
        return Ok(Vec::new());
    }
    if let LpsType::Array { element, len } = ty {
        let element_tys = scalar_ir_types(element)?;
        let mut tys = Vec::new();
        for _ in 0..*len {
            tys.extend(element_tys.iter().copied());
        }
        return Ok(tys);
    }
    if let LpsType::Struct { members, .. } = ty {
        let mut tys = Vec::new();
        for member in members {
            tys.extend(scalar_ir_types(&member.ty)?);
        }
        return Ok(tys);
    }
    let Some(base) = scalar_base_type(ty) else {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("M3 lps-glsl cannot scalarize type {ty:?}"),
        ));
    };
    let lane = match base {
        LpsType::Float => lpir::IrType::F32,
        LpsType::Int | LpsType::UInt | LpsType::Bool => lpir::IrType::I32,
        _ => {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                format!("M3 lps-glsl cannot scalarize type {ty:?}"),
            ));
        }
    };
    Ok(alloc::vec![lane; scalar_lane_count(ty)])
}
