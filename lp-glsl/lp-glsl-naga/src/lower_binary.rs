//! Naga binary operators → LPIR (per-component, with scalar broadcast).

use alloc::format;
use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{BinaryOperator, Expression, Handle, ScalarKind, TypeInner};

use crate::lower_ctx::VRegVec;
use crate::lower_error::LowerError;
use crate::lower_expr::lower_expr_vec;
use crate::naga_util::{expr_scalar_kind, expr_type_inner};

pub(crate) fn lower_binary_vec(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let left_inner = expr_type_inner(ctx.module, ctx.func, left)?;
    let right_inner = expr_type_inner(ctx.module, ctx.func, right)?;
    let lk = expr_scalar_kind(ctx.module, ctx.func, left)?;
    let rk = expr_scalar_kind(ctx.module, ctx.func, right)?;
    if lk != rk {
        return Err(LowerError::UnsupportedExpression(String::from(
            "binary operand kind mismatch",
        )));
    }
    let left_vs = lower_expr_vec(ctx, left)?;
    let right_vs = lower_expr_vec(ctx, right)?;
    let n = left_vs.len().max(right_vs.len());
    if left_vs.len() != right_vs.len() && left_vs.len() != 1 && right_vs.len() != 1 {
        return Err(LowerError::UnsupportedExpression(format!(
            "binary vector width mismatch {} vs {}",
            left_vs.len(),
            right_vs.len()
        )));
    }
    let mut result = VRegVec::new();
    for i in 0..n {
        let l = left_vs[i.min(left_vs.len().saturating_sub(1).max(0))];
        let r = right_vs[i.min(right_vs.len().saturating_sub(1).max(0))];
        let v = lower_binary_scalar(ctx, op, l, r, lk, &left_inner, &right_inner)?;
        result.push(v);
    }
    Ok(result)
}

fn lower_binary_scalar(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
    lk: ScalarKind,
    _left_ty: &TypeInner,
    _right_ty: &TypeInner,
) -> Result<VReg, LowerError> {
    match lk {
        ScalarKind::Float => lower_binary_float(ctx, op, lhs, rhs),
        ScalarKind::Sint => lower_binary_sint(ctx, op, lhs, rhs),
        ScalarKind::Uint => lower_binary_uint(ctx, op, lhs, rhs),
        ScalarKind::Bool => lower_binary_bool(ctx, op, lhs, rhs),
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(LowerError::UnsupportedType(
            String::from("abstract binary op"),
        )),
    }
}

fn lower_binary_float(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    if op == BinaryOperator::Modulo {
        return lower_float_modulo(ctx, lhs, rhs);
    }
    let dst_ty = match op {
        BinaryOperator::Equal
        | BinaryOperator::NotEqual
        | BinaryOperator::Less
        | BinaryOperator::LessEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterEqual => IrType::I32,
        _ => IrType::F32,
    };
    let dst = ctx.fb.alloc_vreg(dst_ty);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Fadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Fsub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Fmul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::Fdiv { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Feq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Fne { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::Flt { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::Fle { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::Fgt { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::Fge { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported float binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_float_modulo(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    x: VReg,
    y: VReg,
) -> Result<VReg, LowerError> {
    let v_div = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fdiv {
        dst: v_div,
        lhs: x,
        rhs: y,
    });
    let v_fl = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Ffloor {
        dst: v_fl,
        src: v_div,
    });
    let v_mul = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fmul {
        dst: v_mul,
        lhs: v_fl,
        rhs: y,
    });
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsub {
        dst,
        lhs: x,
        rhs: v_mul,
    });
    Ok(dst)
}

fn lower_binary_sint(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Iadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Isub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Imul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::IdivS { dst, lhs, rhs }),
        BinaryOperator::Modulo => ctx.fb.push(Op::IremS { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::IltS { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::IleS { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::IgtS { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::IgeS { dst, lhs, rhs }),
        BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::InclusiveOr => ctx.fb.push(Op::Ior { dst, lhs, rhs }),
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::ShiftLeft => ctx.fb.push(Op::Ishl { dst, lhs, rhs }),
        BinaryOperator::ShiftRight => ctx.fb.push(Op::IshrS { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported sint binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_binary_uint(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::Add => ctx.fb.push(Op::Iadd { dst, lhs, rhs }),
        BinaryOperator::Subtract => ctx.fb.push(Op::Isub { dst, lhs, rhs }),
        BinaryOperator::Multiply => ctx.fb.push(Op::Imul { dst, lhs, rhs }),
        BinaryOperator::Divide => ctx.fb.push(Op::IdivU { dst, lhs, rhs }),
        BinaryOperator::Modulo => ctx.fb.push(Op::IremU { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        BinaryOperator::Less => ctx.fb.push(Op::IltU { dst, lhs, rhs }),
        BinaryOperator::LessEqual => ctx.fb.push(Op::IleU { dst, lhs, rhs }),
        BinaryOperator::Greater => ctx.fb.push(Op::IgtU { dst, lhs, rhs }),
        BinaryOperator::GreaterEqual => ctx.fb.push(Op::IgeU { dst, lhs, rhs }),
        BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::InclusiveOr => ctx.fb.push(Op::Ior { dst, lhs, rhs }),
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::ShiftLeft => ctx.fb.push(Op::Ishl { dst, lhs, rhs }),
        BinaryOperator::ShiftRight => ctx.fb.push(Op::IshrU { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported uint binary {op:?}"
            )));
        }
    }
    Ok(dst)
}

fn lower_binary_bool(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: BinaryOperator,
    lhs: VReg,
    rhs: VReg,
) -> Result<VReg, LowerError> {
    let dst = ctx.fb.alloc_vreg(IrType::I32);
    match op {
        BinaryOperator::LogicalAnd | BinaryOperator::And => ctx.fb.push(Op::Iand { dst, lhs, rhs }),
        BinaryOperator::LogicalOr | BinaryOperator::InclusiveOr => {
            ctx.fb.push(Op::Ior { dst, lhs, rhs })
        }
        BinaryOperator::ExclusiveOr => ctx.fb.push(Op::Ixor { dst, lhs, rhs }),
        BinaryOperator::Equal => ctx.fb.push(Op::Ieq { dst, lhs, rhs }),
        BinaryOperator::NotEqual => ctx.fb.push(Op::Ine { dst, lhs, rhs }),
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported bool binary {op:?}"
            )));
        }
    }
    Ok(dst)
}
