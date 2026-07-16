use alloc::format;
use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::arena::{ExprId, ExprList, HirArena};
use super::coerce::{
    coerce_arithmetic_pair, coerce_comparison_pair, coerce_expr, vector_dominant_type,
};
use super::scalar::{scalar_base_type, scalar_lane_count};
use super::types::BuiltinKind;

pub(crate) fn builtin_kind(name: &str) -> Option<BuiltinKind> {
    Some(match name {
        "abs" => BuiltinKind::Abs,
        "all" => BuiltinKind::All,
        "any" => BuiltinKind::Any,
        "bitCount" => BuiltinKind::BitCount,
        "bitfieldExtract" => BuiltinKind::BitfieldExtract,
        "bitfieldInsert" => BuiltinKind::BitfieldInsert,
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
        "imulExtended" => BuiltinKind::ImulExtended,
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
        "modf" => BuiltinKind::Modf,
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
        "uaddCarry" => BuiltinKind::UaddCarry,
        "umulExtended" => BuiltinKind::UmulExtended,
        "usubBorrow" => BuiltinKind::UsubBorrow,
        _ => return None,
    })
}

pub(crate) fn builtin_has_out_args(kind: BuiltinKind) -> bool {
    matches!(
        kind,
        BuiltinKind::ImulExtended
            | BuiltinKind::Modf
            | BuiltinKind::UaddCarry
            | BuiltinKind::UmulExtended
            | BuiltinKind::UsubBorrow
    )
}

pub(crate) fn is_glsl_import(name: &str) -> bool {
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
    arena: &mut HirArena,
    span: Span,
    name: &str,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
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
        let arg = one_arg(arena, span, args)?;
        let arg_ty = arena.expr_ty(arg).clone();
        let arg_base = scalar_base_type(&arg_ty).unwrap_or_else(|| arg_ty.clone());
        if arg_base == LpsType::Float {
            return Ok((arena.push_expr_list([arg]), arg_ty));
        }
        let arg = coerce_expr(arena, arg, &LpsType::Float)?;
        return Ok((arena.push_expr_list([arg]), LpsType::Float));
    }
    if name == "pow" && args.len() == 2 {
        let (a, b) = two_args(arena, span, args)?;
        let (a, b, ty) = coerce_arithmetic_pair(arena, span, a, b)?;
        if scalar_base_type(&ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "pow expects float lanes"));
        }
        return Ok((arena.push_expr_list([a, b]), ty));
    }
    if name == "atan" && args.len() == 2 {
        let (a, b) = two_args(arena, span, args)?;
        let (a, b, ty) = coerce_arithmetic_pair(arena, span, a, b)?;
        if scalar_base_type(&ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "atan expects float lanes"));
        }
        return Ok((arena.push_expr_list([a, b]), ty));
    }
    if name == "ldexp" && args.len() == 2 {
        let (x, exp) = two_args(arena, span, args)?;
        let x_ty = arena.expr_ty(x).clone();
        if x_ty.is_matrix() || scalar_base_type(&x_ty) != Some(LpsType::Float) {
            return Err(Diagnostic::error(span, "ldexp expects float lanes"));
        }
        let exp_ty = match scalar_lane_count(&x_ty) {
            1 => LpsType::Int,
            2 => LpsType::IVec2,
            3 => LpsType::IVec3,
            4 => LpsType::IVec4,
            _ => return Err(Diagnostic::error(span, "unsupported ldexp vector width")),
        };
        let exp = coerce_expr(arena, exp, &exp_ty)?;
        return Ok((arena.push_expr_list([x, exp]), x_ty));
    }

    Err(Diagnostic::error(
        span,
        format!("unsupported GLSL import signature `{name}`"),
    ))
}

pub(super) fn type_builtin_args(
    arena: &mut HirArena,
    span: Span,
    kind: BuiltinKind,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    if builtin_has_out_args(kind) {
        return Err(Diagnostic::error(
            span,
            "builtin with out arguments requires lvalue-aware typing",
        ));
    }
    check_builtin_arity(span, kind, args.len())?;
    match kind {
        BuiltinKind::Abs | BuiltinKind::Floor | BuiltinKind::Fract => type_passthrough(arena, args),
        BuiltinKind::BitCount
        | BuiltinKind::BitfieldReverse
        | BuiltinKind::FindLsb
        | BuiltinKind::FindMsb => type_integer_lane_builtin(arena, span, args),
        BuiltinKind::BitfieldExtract => type_bitfield_extract(arena, span, args),
        BuiltinKind::BitfieldInsert => type_bitfield_insert(arena, span, args),
        BuiltinKind::Ceil
        | BuiltinKind::Degrees
        | BuiltinKind::InverseSqrt
        | BuiltinKind::Radians
        | BuiltinKind::Round
        | BuiltinKind::RoundEven
        | BuiltinKind::Sqrt
        | BuiltinKind::Trunc => type_float_lane_builtin(arena, span, args),
        BuiltinKind::Sign => type_sign_builtin(arena, span, args),
        BuiltinKind::IsInf | BuiltinKind::IsNan => type_float_predicate_builtin(arena, span, args),
        BuiltinKind::Length | BuiltinKind::Normalize => {
            type_length_or_normalize(arena, span, kind, args)
        }
        BuiltinKind::Determinant | BuiltinKind::Inverse | BuiltinKind::Transpose => {
            type_matrix_builtin(arena, span, kind, args)
        }
        BuiltinKind::Cross => type_cross_builtin(arena, span, args),
        BuiltinKind::Distance | BuiltinKind::Dot => type_distance_or_dot(arena, span, args),
        BuiltinKind::All | BuiltinKind::Any => type_all_or_any(arena, span, args),
        BuiltinKind::Not => type_not_builtin(arena, span, args),
        BuiltinKind::Max | BuiltinKind::Min | BuiltinKind::Mod => {
            type_arithmetic_pair_builtin(arena, span, args)
        }
        BuiltinKind::MatrixCompMult => type_matrix_comp_mult(arena, span, args),
        BuiltinKind::OuterProduct => type_outer_product(arena, span, args),
        BuiltinKind::Equal
        | BuiltinKind::GreaterThan
        | BuiltinKind::GreaterThanEqual
        | BuiltinKind::LessThan
        | BuiltinKind::LessThanEqual
        | BuiltinKind::NotEqual => type_relational_builtin(arena, span, args),
        BuiltinKind::Clamp | BuiltinKind::Smoothstep => type_clamp_or_smoothstep(arena, span, args),
        BuiltinKind::Fma => type_fma_builtin(arena, span, args),
        BuiltinKind::Mix => type_mix_builtin(arena, span, args),
        BuiltinKind::ImulExtended
        | BuiltinKind::Modf
        | BuiltinKind::UaddCarry
        | BuiltinKind::UmulExtended
        | BuiltinKind::UsubBorrow => unreachable!("out-arg builtins return before type checks"),
    }
}

pub(crate) fn check_builtin_arity(
    span: Span,
    kind: BuiltinKind,
    actual: usize,
) -> Result<(), Diagnostic> {
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
        BuiltinKind::BitfieldExtract
        | BuiltinKind::Clamp
        | BuiltinKind::Fma
        | BuiltinKind::Mix
        | BuiltinKind::Smoothstep => 3,
        BuiltinKind::BitfieldInsert => 4,
        BuiltinKind::ImulExtended
        | BuiltinKind::Modf
        | BuiltinKind::UaddCarry
        | BuiltinKind::UmulExtended
        | BuiltinKind::UsubBorrow => unreachable!("out-arg builtins return before arity checks"),
    };
    if actual != arity {
        return Err(Diagnostic::error(
            span,
            format!("builtin expects {arity} arguments"),
        ));
    }
    Ok(())
}

fn type_passthrough(arena: &HirArena, args: ExprList) -> Result<(ExprList, LpsType), Diagnostic> {
    let ty = arena.expr_ty(arena.expr_list(args)[0]).clone();
    Ok((args, ty))
}

fn type_integer_lane_builtin(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ty = arena.expr_ty(arena.expr_list(args)[0]).clone();
    if ty.is_matrix() || !matches!(scalar_base_type(&ty), Some(LpsType::Int | LpsType::UInt)) {
        return Err(Diagnostic::error(
            span,
            "integer builtin expects int/uint scalar/vector lanes",
        ));
    }
    Ok((args, ty))
}

fn type_bitfield_extract(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ids = arena.expr_list(args);
    let value_ty = arena.expr_ty(ids[0]).clone();
    if value_ty.is_matrix()
        || !matches!(
            scalar_base_type(&value_ty),
            Some(LpsType::Int | LpsType::UInt)
        )
        || *arena.expr_ty(ids[1]) != LpsType::Int
        || *arena.expr_ty(ids[2]) != LpsType::Int
    {
        return Err(Diagnostic::error(
            span,
            "bitfieldExtract expects int/uint lanes and int offset/bits",
        ));
    }
    Ok((args, value_ty))
}

fn type_bitfield_insert(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (base, insert, offset, bits) = four_args(arena, span, args)?;
    let offset_ty = arena.expr_ty(offset).clone();
    let bits_ty = arena.expr_ty(bits).clone();
    let (base, insert, value_ty) = coerce_arithmetic_pair(arena, span, base, insert)?;
    if value_ty.is_matrix()
        || !matches!(
            scalar_base_type(&value_ty),
            Some(LpsType::Int | LpsType::UInt)
        )
        || offset_ty != LpsType::Int
        || bits_ty != LpsType::Int
    {
        return Err(Diagnostic::error(
            span,
            "bitfieldInsert expects int/uint lanes and int offset/bits",
        ));
    }
    Ok((arena.push_expr_list([base, insert, offset, bits]), value_ty))
}

fn type_float_lane_builtin(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ty = arena.expr_ty(arena.expr_list(args)[0]);
    if ty.is_matrix() || scalar_base_type(ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(span, "builtin expects float lanes"));
    }
    type_passthrough(arena, args)
}

fn type_sign_builtin(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ty = arena.expr_ty(arena.expr_list(args)[0]).clone();
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

fn type_float_predicate_builtin(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let arg_ty = arena.expr_ty(arena.expr_list(args)[0]);
    if arg_ty.is_matrix() || scalar_base_type(arg_ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(
            span,
            "builtin expects float scalar/vector lanes",
        ));
    }
    let ty = match scalar_lane_count(arg_ty) {
        1 => LpsType::Bool,
        2 => LpsType::BVec2,
        3 => LpsType::BVec3,
        4 => LpsType::BVec4,
        _ => return Err(Diagnostic::error(span, "unsupported builtin vector width")),
    };
    Ok((args, ty))
}

fn type_length_or_normalize(
    arena: &HirArena,
    span: Span,
    kind: BuiltinKind,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let arg_ty = arena.expr_ty(arena.expr_list(args)[0]);
    if scalar_base_type(arg_ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(span, "builtin expects float lanes"));
    }
    let ty = match kind {
        BuiltinKind::Length => LpsType::Float,
        BuiltinKind::Normalize => arg_ty.clone(),
        _ => unreachable!(),
    };
    Ok((args, ty))
}

fn type_matrix_builtin(
    arena: &HirArena,
    span: Span,
    kind: BuiltinKind,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let arg_ty = arena.expr_ty(arena.expr_list(args)[0]);
    if !arg_ty.is_matrix() {
        let message = match kind {
            BuiltinKind::Determinant => "determinant expects a matrix",
            BuiltinKind::Inverse => "inverse expects a matrix",
            BuiltinKind::Transpose => "transpose expects a matrix",
            _ => unreachable!(),
        };
        return Err(Diagnostic::error(span, message));
    }
    let ty = match kind {
        BuiltinKind::Determinant => LpsType::Float,
        BuiltinKind::Inverse | BuiltinKind::Transpose => arg_ty.clone(),
        _ => unreachable!(),
    };
    Ok((args, ty))
}

fn type_cross_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b) = two_args(arena, span, args)?;
    let a = coerce_expr(arena, a, &LpsType::Vec3)?;
    let b = coerce_expr(arena, b, &LpsType::Vec3)?;
    Ok((arena.push_expr_list([a, b]), LpsType::Vec3))
}

fn type_distance_or_dot(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b) = two_args(arena, span, args)?;
    let (a, b, ty) = coerce_arithmetic_pair(arena, span, a, b)?;
    if scalar_base_type(&ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(span, "builtin expects float lanes"));
    }
    Ok((arena.push_expr_list([a, b]), LpsType::Float))
}

fn type_all_or_any(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let arg = one_arg(arena, span, args)?;
    let ty = arena.expr_ty(arg).clone();
    let arg = coerce_expr(arena, arg, &ty)?;
    if scalar_base_type(arena.expr_ty(arg)) != Some(LpsType::Bool) {
        return Err(Diagnostic::error(span, "all/any expects bool lanes"));
    }
    Ok((arena.push_expr_list([arg]), LpsType::Bool))
}

fn type_not_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let arg = one_arg(arena, span, args)?;
    let ty = arena.expr_ty(arg).clone();
    if scalar_base_type(&ty) != Some(LpsType::Bool) {
        return Err(Diagnostic::error(span, "not expects bool lanes"));
    }
    Ok((arena.push_expr_list([arg]), ty))
}

fn type_arithmetic_pair_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b) = two_args(arena, span, args)?;
    let (a, b, ty) = coerce_arithmetic_pair(arena, span, a, b)?;
    Ok((arena.push_expr_list([a, b]), ty))
}

fn type_matrix_comp_mult(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ids = arena.expr_list(args);
    let lhs_ty = arena.expr_ty(ids[0]);
    let rhs_ty = arena.expr_ty(ids[1]);
    if !lhs_ty.is_matrix() || lhs_ty != rhs_ty {
        return Err(Diagnostic::error(
            span,
            "matrixCompMult expects matching matrix operands",
        ));
    }
    Ok((args, lhs_ty.clone()))
}

fn type_outer_product(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let ids = arena.expr_list(args);
    let a_ty = arena.expr_ty(ids[0]);
    let b_ty = arena.expr_ty(ids[1]);
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

fn type_relational_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b) = two_args(arena, span, args)?;
    let (a, b, ty) = coerce_comparison_pair(arena, span, a, b)?;
    Ok((arena.push_expr_list([a, b]), ty))
}

fn type_clamp_or_smoothstep(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b, c) = three_args(arena, span, args)?;
    let (a, b, ty_ab) = coerce_arithmetic_pair(arena, span, a, b)?;
    let c = match coerce_expr(arena, c, &ty_ab) {
        Ok(c) => c,
        Err(_) => {
            let (_, c, _) = coerce_arithmetic_pair(arena, span, a, c)?;
            c
        }
    };
    let ty = vector_dominant_type(&[arena.expr_ty(a), arena.expr_ty(b), arena.expr_ty(c)])
        .ok_or_else(|| Diagnostic::error(span, "unsupported builtin argument types"))?;
    let a = coerce_expr(arena, a, &ty)?;
    let b = coerce_expr(arena, b, &ty)?;
    let c = coerce_expr(arena, c, &ty)?;
    Ok((arena.push_expr_list([a, b, c]), ty))
}

fn type_fma_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (a, b, c) = three_args(arena, span, args)?;
    let (a, b, ty) = coerce_arithmetic_pair(arena, span, a, b)?;
    if ty.is_matrix() || scalar_base_type(&ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(
            span,
            "fma expects float scalar/vector lanes",
        ));
    }
    let c = coerce_expr(arena, c, &ty)?;
    Ok((arena.push_expr_list([a, b, c]), ty))
}

fn type_mix_builtin(
    arena: &mut HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprList, LpsType), Diagnostic> {
    let (x, y, a) = three_args(arena, span, args)?;
    let (x, y, ty) = coerce_arithmetic_pair(arena, span, x, y)?;
    let a = if scalar_lane_count(arena.expr_ty(a)) == 1 {
        coerce_expr(arena, a, &LpsType::Float)?
    } else {
        coerce_expr(arena, a, &ty)?
    };
    Ok((arena.push_expr_list([x, y, a]), ty))
}

fn one_arg(arena: &HirArena, span: Span, args: ExprList) -> Result<ExprId, Diagnostic> {
    match arena.expr_list(args) {
        [arg] => Ok(*arg),
        _ => Err(Diagnostic::error(span, "expected one argument")),
    }
}

fn two_args(arena: &HirArena, span: Span, args: ExprList) -> Result<(ExprId, ExprId), Diagnostic> {
    match arena.expr_list(args) {
        [a, b] => Ok((*a, *b)),
        _ => Err(Diagnostic::error(span, "expected two arguments")),
    }
}

fn three_args(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprId, ExprId, ExprId), Diagnostic> {
    match arena.expr_list(args) {
        [a, b, c] => Ok((*a, *b, *c)),
        _ => Err(Diagnostic::error(span, "expected three arguments")),
    }
}

fn four_args(
    arena: &HirArena,
    span: Span,
    args: ExprList,
) -> Result<(ExprId, ExprId, ExprId, ExprId), Diagnostic> {
    match arena.expr_list(args) {
        [a, b, c, d] => Ok((*a, *b, *c, *d)),
        _ => Err(Diagnostic::error(span, "expected four arguments")),
    }
}
