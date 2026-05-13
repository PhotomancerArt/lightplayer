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
        "bitCount" => BuiltinKind::BitCount,
        "bitfieldReverse" => BuiltinKind::BitfieldReverse,
        "ceil" => BuiltinKind::Ceil,
        "clamp" => BuiltinKind::Clamp,
        "cross" => BuiltinKind::Cross,
        "degrees" => BuiltinKind::Degrees,
        "determinant" => BuiltinKind::Determinant,
        "distance" => BuiltinKind::Distance,
        "dot" => BuiltinKind::Dot,
        "equal" => BuiltinKind::Equal,
        "floor" => BuiltinKind::Floor,
        "fma" => BuiltinKind::Fma,
        "findLSB" => BuiltinKind::FindLsb,
        "findMSB" => BuiltinKind::FindMsb,
        "fract" => BuiltinKind::Fract,
        "greaterThan" => BuiltinKind::GreaterThan,
        "greaterThanEqual" => BuiltinKind::GreaterThanEqual,
        "inverse" => BuiltinKind::Inverse,
        "inversesqrt" => BuiltinKind::InverseSqrt,
        "isinf" => BuiltinKind::IsInf,
        "isnan" => BuiltinKind::IsNan,
        "length" => BuiltinKind::Length,
        "lessThan" => BuiltinKind::LessThan,
        "lessThanEqual" => BuiltinKind::LessThanEqual,
        "matrixCompMult" => BuiltinKind::MatrixCompMult,
        "max" => BuiltinKind::Max,
        "min" => BuiltinKind::Min,
        "mix" => BuiltinKind::Mix,
        "mod" => BuiltinKind::Mod,
        "not" => BuiltinKind::Not,
        "normalize" => BuiltinKind::Normalize,
        "notEqual" => BuiltinKind::NotEqual,
        "outerProduct" => BuiltinKind::OuterProduct,
        "radians" => BuiltinKind::Radians,
        "round" => BuiltinKind::Round,
        "roundEven" => BuiltinKind::RoundEven,
        "sign" => BuiltinKind::Sign,
        "smoothstep" => BuiltinKind::Smoothstep,
        "sqrt" => BuiltinKind::Sqrt,
        "transpose" => BuiltinKind::Transpose,
        "trunc" => BuiltinKind::Trunc,
        _ => return None,
    })
}

pub(super) fn is_glsl_import(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
            | "exp"
            | "exp2"
            | "log"
            | "log2"
            | "ldexp"
            | "pow"
    )
}

pub(super) fn type_glsl_import_args(
    span: Span,
    name: &str,
    args: Vec<HirExpr>,
) -> Result<(Vec<HirExpr>, LpsType), Diagnostic> {
    if matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
            | "exp"
            | "exp2"
            | "log"
            | "log2"
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
    if name == "atan" && args.len() == 2 {
        let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
        if scalar_base_type(&ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "atan expects float lanes"));
        }
        return Ok((alloc::vec![a, b], ty));
    }
    if name == "ldexp" && args.len() == 2 {
        let x = args[0].clone();
        if x.ty.is_matrix() || scalar_base_type(&x.ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "ldexp expects float lanes"));
        }
        let exp_ty = match scalar_lane_count(&x.ty) {
            1 => LpsType::Int,
            2 => LpsType::IVec2,
            3 => LpsType::IVec3,
            4 => LpsType::IVec4,
            _ => return Err(Diagnostic::error(span, "unsupported ldexp vector width")),
        };
        let exp = coerce_expr(args[1].clone(), &exp_ty)?;
        let ty = x.ty.clone();
        return Ok((alloc::vec![x, exp], ty));
    }

    Err(Diagnostic::error(
        span,
        format!("unsupported GLSL import signature `{name}`"),
    ))
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
        | BuiltinKind::BitCount
        | BuiltinKind::BitfieldReverse
        | BuiltinKind::Ceil
        | BuiltinKind::Degrees
        | BuiltinKind::Determinant
        | BuiltinKind::Floor
        | BuiltinKind::FindLsb
        | BuiltinKind::FindMsb
        | BuiltinKind::Fract
        | BuiltinKind::Inverse
        | BuiltinKind::InverseSqrt
        | BuiltinKind::IsInf
        | BuiltinKind::IsNan
        | BuiltinKind::Length
        | BuiltinKind::Normalize
        | BuiltinKind::Not
        | BuiltinKind::Radians
        | BuiltinKind::Round
        | BuiltinKind::RoundEven
        | BuiltinKind::Sign
        | BuiltinKind::Sqrt
        | BuiltinKind::Transpose
        | BuiltinKind::Trunc => 1,
        BuiltinKind::Equal
        | BuiltinKind::Cross
        | BuiltinKind::Distance
        | BuiltinKind::Dot
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::MatrixCompMult
        | BuiltinKind::Max
        | BuiltinKind::Min
        | BuiltinKind::Mod
        | BuiltinKind::NotEqual
        | BuiltinKind::OuterProduct => 2,
        BuiltinKind::Clamp | BuiltinKind::Fma | BuiltinKind::Mix | BuiltinKind::Smoothstep => 3,
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
        BuiltinKind::BitCount
        | BuiltinKind::BitfieldReverse
        | BuiltinKind::FindLsb
        | BuiltinKind::FindMsb => {
            let ty = args[0].ty.clone();
            if ty.is_matrix()
                || !matches!(scalar_base_type(&ty), Some(LpsType::Int | LpsType::UInt))
            {
                return Err(Diagnostic::error(
                    span,
                    "integer builtin expects int/uint scalar/vector lanes",
                ));
            }
            Ok((args, ty))
        }
        BuiltinKind::Ceil
        | BuiltinKind::Degrees
        | BuiltinKind::InverseSqrt
        | BuiltinKind::Radians
        | BuiltinKind::Round
        | BuiltinKind::RoundEven
        | BuiltinKind::Sqrt
        | BuiltinKind::Trunc => {
            if args[0].ty.is_matrix() || scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(span, "builtin expects float lanes"));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Sign => {
            let ty = args[0].ty.clone();
            if ty.is_matrix()
                || !matches!(
                    scalar_base_type(&ty),
                    Some(LpsType::Float | LpsType::Int | LpsType::UInt)
                )
            {
                return Err(Diagnostic::error(
                    span,
                    "sign expects numeric scalar/vector lanes",
                ));
            }
            Ok((args, ty))
        }
        BuiltinKind::IsInf | BuiltinKind::IsNan => {
            if args[0].ty.is_matrix() || scalar_base_type(&args[0].ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(
                    span,
                    "builtin expects float scalar/vector lanes",
                ));
            }
            let ty = match scalar_lane_count(&args[0].ty) {
                1 => LpsType::Bool,
                2 => LpsType::BVec2,
                3 => LpsType::BVec3,
                4 => LpsType::BVec4,
                _ => return Err(Diagnostic::error(span, "unsupported builtin vector width")),
            };
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
        BuiltinKind::Determinant => {
            if !args[0].ty.is_matrix() {
                return Err(Diagnostic::error(span, "determinant expects a matrix"));
            }
            Ok((args, LpsType::Float))
        }
        BuiltinKind::Inverse => {
            if !args[0].ty.is_matrix() {
                return Err(Diagnostic::error(span, "inverse expects a matrix"));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Transpose => {
            if !args[0].ty.is_matrix() {
                return Err(Diagnostic::error(span, "transpose expects a matrix"));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::Cross => {
            let a = coerce_expr(args[0].clone(), &LpsType::Vec3)?;
            let b = coerce_expr(args[1].clone(), &LpsType::Vec3)?;
            Ok((alloc::vec![a, b], LpsType::Vec3))
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
        BuiltinKind::MatrixCompMult => {
            if !args[0].ty.is_matrix() || args[0].ty != args[1].ty {
                return Err(Diagnostic::error(
                    span,
                    "matrixCompMult expects matching matrix operands",
                ));
            }
            let ty = args[0].ty.clone();
            Ok((args, ty))
        }
        BuiltinKind::OuterProduct => {
            let a_ty = &args[0].ty;
            let b_ty = &args[1].ty;
            if scalar_base_type(a_ty) != Some(LpsType::Float)
                || scalar_base_type(b_ty) != Some(LpsType::Float)
                || !a_ty.is_vector()
                || !b_ty.is_vector()
            {
                return Err(Diagnostic::error(
                    span,
                    "outerProduct expects float vector operands",
                ));
            }
            let Some(a_width) = a_ty.component_count() else {
                return Err(Diagnostic::error(span, "outerProduct expects vectors"));
            };
            let Some(b_width) = b_ty.component_count() else {
                return Err(Diagnostic::error(span, "outerProduct expects vectors"));
            };
            if a_width != b_width {
                return Err(Diagnostic::error(
                    span,
                    "lps-glsl currently supports square outerProduct results only",
                ));
            }
            let ty = match a_width {
                2 => LpsType::Mat2,
                3 => LpsType::Mat3,
                4 => LpsType::Mat4,
                _ => return Err(Diagnostic::error(span, "unsupported outerProduct shape")),
            };
            Ok((args, ty))
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
        BuiltinKind::Fma => {
            let (a, b, ty) = coerce_arithmetic_pair(span, args[0].clone(), args[1].clone())?;
            if ty.is_matrix() || scalar_base_type(&ty) != Some(LpsType::Float) {
                return Err(Diagnostic::error(
                    span,
                    "fma expects float scalar/vector lanes",
                ));
            }
            let c = coerce_expr(args[2].clone(), &ty)?;
            Ok((alloc::vec![a, b, c], ty))
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
