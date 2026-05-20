use lps_shared::LpsType;

use crate::Span;
use crate::body::{BinaryOp, UnaryOp};

use super::types::{BuiltinKind, HirExpr, HirExprKind};

pub(super) fn fold_unary(span: Span, op: UnaryOp, expr: &HirExpr) -> Option<HirExpr> {
    match (op, &expr.kind) {
        (UnaryOp::Neg, HirExprKind::FloatLiteral(value)) => Some(float_literal(span, -*value)),
        (UnaryOp::Neg, HirExprKind::IntLiteral(value)) => Some(int_literal(span, -*value)),
        (UnaryOp::Not, HirExprKind::BoolLiteral(value)) => Some(bool_literal(span, !*value)),
        _ => None,
    }
}

pub(super) fn fold_binary(
    span: Span,
    op: BinaryOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
) -> Option<HirExpr> {
    fold_float_binary(span, op, lhs, rhs)
        .or_else(|| fold_int_binary(span, op, lhs, rhs))
        .or_else(|| fold_uint_binary(span, op, lhs, rhs))
        .or_else(|| fold_bool_binary(span, op, lhs, rhs))
}

pub(super) fn fold_builtin_call(
    span: Span,
    kind: BuiltinKind,
    args: &[HirExpr],
    result_ty: &LpsType,
) -> Option<HirExpr> {
    if *result_ty != LpsType::Float {
        return None;
    }
    let value = match kind {
        BuiltinKind::Abs => abs_f32(float_arg(args, 0)?),
        BuiltinKind::Ceil => ceil_f32(float_arg(args, 0)?),
        BuiltinKind::Clamp => {
            let value = float_arg(args, 0)?;
            let lo = float_arg(args, 1)?;
            let hi = float_arg(args, 2)?;
            min_f32(max_f32(value, lo), hi)
        }
        BuiltinKind::Degrees => float_arg(args, 0)? * (180.0 / core::f32::consts::PI),
        BuiltinKind::Floor => floor_f32(float_arg(args, 0)?),
        BuiltinKind::Fract => {
            let value = float_arg(args, 0)?;
            value - floor_f32(value)
        }
        BuiltinKind::Max => max_f32(float_arg(args, 0)?, float_arg(args, 1)?),
        BuiltinKind::Min => min_f32(float_arg(args, 0)?, float_arg(args, 1)?),
        BuiltinKind::Mix => {
            let x = float_arg(args, 0)?;
            let y = float_arg(args, 1)?;
            let a = float_arg(args, 2)?;
            x * (1.0 - a) + y * a
        }
        BuiltinKind::Mod => {
            let rhs = float_arg(args, 1)?;
            if rhs == 0.0 {
                return None;
            }
            let lhs = float_arg(args, 0)?;
            lhs - rhs * floor_f32(lhs / rhs)
        }
        BuiltinKind::Radians => float_arg(args, 0)? * (core::f32::consts::PI / 180.0),
        BuiltinKind::Round => round_f32(float_arg(args, 0)?),
        BuiltinKind::RoundEven => round_even_f32(float_arg(args, 0)?),
        BuiltinKind::Sign => sign_f32(float_arg(args, 0)?),
        BuiltinKind::Trunc => trunc_f32(float_arg(args, 0)?),
        _ => return None,
    };
    Some(float_literal(span, value))
}

pub(super) fn fold_glsl_import_call(
    span: Span,
    name: &str,
    args: &[HirExpr],
    result_ty: &LpsType,
) -> Option<HirExpr> {
    if *result_ty != LpsType::Float || name != "pow" {
        return None;
    }
    let base = float_arg(args, 0)?;
    let exponent = float_arg(args, 1)?;
    if base < 0.0 || (base == 0.0 && exponent <= 0.0) {
        return None;
    }
    let exponent_i = exponent as i32;
    if exponent_i as f32 != exponent || exponent_i.unsigned_abs() > 16 {
        return None;
    }
    let mut result = 1.0;
    for _ in 0..exponent_i.unsigned_abs() {
        result *= base;
    }
    if exponent_i < 0 {
        if result == 0.0 {
            return None;
        }
        result = 1.0 / result;
    }
    Some(float_literal(span, result))
}

pub(super) fn fold_cast(span: Span, target: &LpsType, expr: &HirExpr) -> Option<HirExpr> {
    match (target, &expr.kind) {
        (LpsType::Float, HirExprKind::IntLiteral(value)) => {
            Some(float_literal(span, *value as f32))
        }
        (LpsType::Float, HirExprKind::UIntLiteral(value)) => {
            Some(float_literal(span, *value as f32))
        }
        (LpsType::Float, HirExprKind::BoolLiteral(value)) => {
            Some(float_literal(span, if *value { 1.0 } else { 0.0 }))
        }
        (LpsType::Int, HirExprKind::FloatLiteral(value)) => Some(int_literal(span, *value as i32)),
        (LpsType::Int, HirExprKind::UIntLiteral(value)) => Some(int_literal(span, *value as i32)),
        (LpsType::Int, HirExprKind::BoolLiteral(value)) => {
            Some(int_literal(span, if *value { 1 } else { 0 }))
        }
        (LpsType::UInt, HirExprKind::FloatLiteral(value)) => {
            Some(uint_literal(span, *value as u32))
        }
        (LpsType::UInt, HirExprKind::IntLiteral(value)) => Some(uint_literal(span, *value as u32)),
        (LpsType::UInt, HirExprKind::BoolLiteral(value)) => {
            Some(uint_literal(span, if *value { 1 } else { 0 }))
        }
        (LpsType::Bool, HirExprKind::FloatLiteral(value)) => {
            Some(bool_literal(span, *value != 0.0))
        }
        (LpsType::Bool, HirExprKind::IntLiteral(value)) => Some(bool_literal(span, *value != 0)),
        (LpsType::Bool, HirExprKind::UIntLiteral(value)) => Some(bool_literal(span, *value != 0)),
        _ => None,
    }
}

fn fold_float_binary(span: Span, op: BinaryOp, lhs: &HirExpr, rhs: &HirExpr) -> Option<HirExpr> {
    let (HirExprKind::FloatLiteral(lhs), HirExprKind::FloatLiteral(rhs)) = (&lhs.kind, &rhs.kind)
    else {
        return None;
    };
    let value = match op {
        BinaryOp::Add => lhs + rhs,
        BinaryOp::Sub => lhs - rhs,
        BinaryOp::Mul => lhs * rhs,
        BinaryOp::Div if *rhs != 0.0 => lhs / rhs,
        BinaryOp::Mod if *rhs != 0.0 => lhs % rhs,
        BinaryOp::Lt => return Some(bool_literal(span, lhs < rhs)),
        BinaryOp::Le => return Some(bool_literal(span, lhs <= rhs)),
        BinaryOp::Gt => return Some(bool_literal(span, lhs > rhs)),
        BinaryOp::Ge => return Some(bool_literal(span, lhs >= rhs)),
        BinaryOp::Eq => return Some(bool_literal(span, lhs == rhs)),
        BinaryOp::Ne => return Some(bool_literal(span, lhs != rhs)),
        _ => return None,
    };
    Some(float_literal(span, value))
}

fn fold_int_binary(span: Span, op: BinaryOp, lhs: &HirExpr, rhs: &HirExpr) -> Option<HirExpr> {
    let (HirExprKind::IntLiteral(lhs), HirExprKind::IntLiteral(rhs)) = (&lhs.kind, &rhs.kind)
    else {
        return None;
    };
    let value = match op {
        BinaryOp::Add => lhs.checked_add(*rhs)?,
        BinaryOp::Sub => lhs.checked_sub(*rhs)?,
        BinaryOp::Mul => lhs.checked_mul(*rhs)?,
        BinaryOp::Div if *rhs != 0 => lhs.checked_div(*rhs)?,
        BinaryOp::Mod if *rhs != 0 => lhs.checked_rem(*rhs)?,
        BinaryOp::Lt => return Some(bool_literal(span, lhs < rhs)),
        BinaryOp::Le => return Some(bool_literal(span, lhs <= rhs)),
        BinaryOp::Gt => return Some(bool_literal(span, lhs > rhs)),
        BinaryOp::Ge => return Some(bool_literal(span, lhs >= rhs)),
        BinaryOp::Eq => return Some(bool_literal(span, lhs == rhs)),
        BinaryOp::Ne => return Some(bool_literal(span, lhs != rhs)),
        _ => return None,
    };
    Some(int_literal(span, value))
}

fn fold_uint_binary(span: Span, op: BinaryOp, lhs: &HirExpr, rhs: &HirExpr) -> Option<HirExpr> {
    let (HirExprKind::UIntLiteral(lhs), HirExprKind::UIntLiteral(rhs)) = (&lhs.kind, &rhs.kind)
    else {
        return None;
    };
    let value = match op {
        BinaryOp::Add => lhs.checked_add(*rhs)?,
        BinaryOp::Sub => lhs.checked_sub(*rhs)?,
        BinaryOp::Mul => lhs.checked_mul(*rhs)?,
        BinaryOp::Div if *rhs != 0 => lhs.checked_div(*rhs)?,
        BinaryOp::Mod if *rhs != 0 => lhs.checked_rem(*rhs)?,
        BinaryOp::Lt => return Some(bool_literal(span, lhs < rhs)),
        BinaryOp::Le => return Some(bool_literal(span, lhs <= rhs)),
        BinaryOp::Gt => return Some(bool_literal(span, lhs > rhs)),
        BinaryOp::Ge => return Some(bool_literal(span, lhs >= rhs)),
        BinaryOp::Eq => return Some(bool_literal(span, lhs == rhs)),
        BinaryOp::Ne => return Some(bool_literal(span, lhs != rhs)),
        _ => return None,
    };
    Some(uint_literal(span, value))
}

fn fold_bool_binary(span: Span, op: BinaryOp, lhs: &HirExpr, rhs: &HirExpr) -> Option<HirExpr> {
    let (HirExprKind::BoolLiteral(lhs), HirExprKind::BoolLiteral(rhs)) = (&lhs.kind, &rhs.kind)
    else {
        return None;
    };
    let value = match op {
        BinaryOp::LogicalAnd => *lhs && *rhs,
        BinaryOp::LogicalOr => *lhs || *rhs,
        BinaryOp::LogicalXor => *lhs ^ *rhs,
        BinaryOp::Eq => *lhs == *rhs,
        BinaryOp::Ne => *lhs != *rhs,
        _ => return None,
    };
    Some(bool_literal(span, value))
}

fn float_arg(args: &[HirExpr], index: usize) -> Option<f32> {
    let arg = args.get(index)?;
    if let HirExprKind::FloatLiteral(value) = &arg.kind {
        Some(*value)
    } else {
        None
    }
}

fn float_literal(span: Span, value: f32) -> HirExpr {
    HirExpr {
        span,
        ty: LpsType::Float,
        kind: HirExprKind::FloatLiteral(value),
    }
}

fn int_literal(span: Span, value: i32) -> HirExpr {
    HirExpr {
        span,
        ty: LpsType::Int,
        kind: HirExprKind::IntLiteral(value),
    }
}

fn uint_literal(span: Span, value: u32) -> HirExpr {
    HirExpr {
        span,
        ty: LpsType::UInt,
        kind: HirExprKind::UIntLiteral(value),
    }
}

fn bool_literal(span: Span, value: bool) -> HirExpr {
    HirExpr {
        span,
        ty: LpsType::Bool,
        kind: HirExprKind::BoolLiteral(value),
    }
}

fn abs_f32(value: f32) -> f32 {
    if value < 0.0 { -value } else { value }
}

fn sign_f32(value: f32) -> f32 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn min_f32(lhs: f32, rhs: f32) -> f32 {
    if lhs < rhs { lhs } else { rhs }
}

fn max_f32(lhs: f32, rhs: f32) -> f32 {
    if lhs > rhs { lhs } else { rhs }
}

fn trunc_f32(value: f32) -> f32 {
    (value as i32) as f32
}

fn floor_f32(value: f32) -> f32 {
    let truncated = trunc_f32(value);
    if value < truncated {
        truncated - 1.0
    } else {
        truncated
    }
}

fn ceil_f32(value: f32) -> f32 {
    let truncated = trunc_f32(value);
    if value > truncated {
        truncated + 1.0
    } else {
        truncated
    }
}

fn round_f32(value: f32) -> f32 {
    if value >= 0.0 {
        floor_f32(value + 0.5)
    } else {
        ceil_f32(value - 0.5)
    }
}

fn round_even_f32(value: f32) -> f32 {
    let floor = floor_f32(value);
    let frac = value - floor;
    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        let floor_i = floor as i32;
        if floor_i % 2 == 0 { floor } else { floor + 1.0 }
    }
}
