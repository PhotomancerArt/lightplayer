//! Naga [`naga::Expression`] → LPIR ops (scalar subset).

use alloc::format;
use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{BinaryOperator, Expression, Handle, Literal, ScalarKind, UnaryOperator};

use crate::expr_scalar::expr_scalar_kind;
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;
use crate::lower_math;

pub(crate) fn lower_expr(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VReg, LowerError> {
    let i = expr.index();
    if let Some(v) = ctx.expr_cache.get(i).and_then(|c| *c) {
        return Ok(v);
    }
    let v = lower_expr_uncached(ctx, expr)?;
    if let Some(slot) = ctx.expr_cache.get_mut(i) {
        *slot = Some(v);
    }
    Ok(v)
}

fn lower_expr_uncached(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VReg, LowerError> {
    match &ctx.func.expressions[expr] {
        Expression::Literal(l) => push_literal(&mut ctx.fb, l),
        Expression::Constant(h) => {
            let init = ctx.module.constants[*h].init;
            lower_global_expr(ctx, init)
        }
        Expression::FunctionArgument(i) => Ok(VReg(*i)),
        Expression::LocalVariable(_) => Err(LowerError::UnsupportedExpression(String::from(
            "LocalVariable must be used through Load",
        ))),
        Expression::Load { pointer } => match &ctx.func.expressions[*pointer] {
            Expression::LocalVariable(lv) => ctx.resolve_local(*lv),
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "Load from non-local pointer",
            ))),
        },
        Expression::Binary { op, left, right } => lower_binary(ctx, *op, *left, *right),
        Expression::Unary { op, expr: inner } => lower_unary(ctx, *op, *inner),
        Expression::Select {
            condition,
            accept,
            reject,
        } => lower_select(ctx, *condition, *accept, *reject),
        Expression::As {
            expr: inner,
            kind,
            convert,
        } => {
            if convert.is_some_and(|w| w != 4) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "As with non-32-bit byte convert",
                )));
            }
            lower_as(ctx, *inner, *kind)
        }
        Expression::ZeroValue(ty_h) => lower_zero_value(ctx, *ty_h),
        Expression::CallResult(_) => {
            let i = expr.index();
            ctx.expr_cache.get(i).copied().flatten().ok_or_else(|| {
                LowerError::Internal(String::from(
                    "CallResult used before matching Call statement",
                ))
            })
        }
        Expression::Math {
            fun,
            arg,
            arg1,
            arg2,
            arg3,
        } => lower_math::lower_math(ctx, *fun, *arg, *arg1, *arg2, *arg3),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "{:?}",
            ctx.func.expressions[expr]
        ))),
    }
}

fn lower_global_expr(ctx: &mut LowerCtx<'_>, expr: Handle<Expression>) -> Result<VReg, LowerError> {
    match &ctx.module.global_expressions[expr] {
        Expression::Literal(l) => push_literal(&mut ctx.fb, l),
        _ => Err(LowerError::UnsupportedExpression(format!(
            "unsupported global expression init {expr:?}"
        ))),
    }
}

fn push_literal(fb: &mut lpir::FunctionBuilder, lit: &Literal) -> Result<VReg, LowerError> {
    match *lit {
        Literal::F32(v) => {
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(Op::FconstF32 { dst: d, value: v });
            Ok(d)
        }
        Literal::I32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 { dst: d, value: v });
            Ok(d)
        }
        Literal::U32(v) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 {
                dst: d,
                value: v as i32,
            });
            Ok(d)
        }
        Literal::Bool(b) => {
            let d = fb.alloc_vreg(IrType::I32);
            fb.push(Op::IconstI32 {
                dst: d,
                value: b as i32,
            });
            Ok(d)
        }
        Literal::F64(v) => {
            let f = v as f32;
            let d = fb.alloc_vreg(IrType::F32);
            fb.push(Op::FconstF32 { dst: d, value: f });
            Ok(d)
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "unsupported literal {lit:?}"
        ))),
    }
}

fn lower_zero_value(ctx: &mut LowerCtx<'_>, ty_h: Handle<naga::Type>) -> Result<VReg, LowerError> {
    match &ctx.module.types[ty_h].inner {
        naga::TypeInner::Scalar(scalar) => match scalar.kind {
            ScalarKind::Float => {
                let d = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(Op::FconstF32 { dst: d, value: 0.0 });
                Ok(d)
            }
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
                let d = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::IconstI32 { dst: d, value: 0 });
                Ok(d)
            }
            ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(
                LowerError::UnsupportedType(String::from("abstract zero value")),
            ),
        },
        _ => Err(LowerError::UnsupportedType(String::from(
            "ZeroValue non-scalar",
        ))),
    }
}

fn lower_binary(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
) -> Result<VReg, LowerError> {
    let lk = expr_scalar_kind(ctx.module, ctx.func, left)?;
    let rk = expr_scalar_kind(ctx.module, ctx.func, right)?;
    if lk != rk {
        return Err(LowerError::UnsupportedExpression(String::from(
            "binary operand kind mismatch",
        )));
    }
    let lhs = lower_expr(ctx, left)?;
    let rhs = lower_expr(ctx, right)?;
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
    ctx: &mut LowerCtx<'_>,
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

fn lower_float_modulo(ctx: &mut LowerCtx<'_>, x: VReg, y: VReg) -> Result<VReg, LowerError> {
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
    ctx: &mut LowerCtx<'_>,
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
    ctx: &mut LowerCtx<'_>,
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
    ctx: &mut LowerCtx<'_>,
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

fn lower_unary(
    ctx: &mut LowerCtx<'_>,
    op: UnaryOperator,
    inner: Handle<Expression>,
) -> Result<VReg, LowerError> {
    let k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    let src = lower_expr(ctx, inner)?;
    let dst = match op {
        UnaryOperator::LogicalNot => {
            if k != ScalarKind::Bool {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "logical not on non-bool",
                )));
            }
            ctx.fb.alloc_vreg(IrType::I32)
        }
        UnaryOperator::BitwiseNot => {
            if k != ScalarKind::Sint && k != ScalarKind::Uint {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "bitwise not on non-integer",
                )));
            }
            ctx.fb.alloc_vreg(IrType::I32)
        }
        UnaryOperator::Negate => match k {
            ScalarKind::Float => ctx.fb.alloc_vreg(IrType::F32),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
                ctx.fb.alloc_vreg(IrType::I32)
            }
            ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
                return Err(LowerError::UnsupportedType(String::from("abstract unary")));
            }
        },
    };
    match op {
        UnaryOperator::Negate => match k {
            ScalarKind::Float => ctx.fb.push(Op::Fneg { dst, src }),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => {
                ctx.fb.push(Op::Ineg { dst, src })
            }
            _ => {}
        },
        UnaryOperator::LogicalNot => {
            ctx.fb.push(Op::IeqImm { dst, src, imm: 0 });
        }
        UnaryOperator::BitwiseNot => ctx.fb.push(Op::Ibnot { dst, src }),
    }
    Ok(dst)
}

fn lower_select(
    ctx: &mut LowerCtx<'_>,
    condition: Handle<Expression>,
    accept: Handle<Expression>,
    reject: Handle<Expression>,
) -> Result<VReg, LowerError> {
    let cond = lower_expr(ctx, condition)?;
    let t = lower_expr(ctx, accept)?;
    let f = lower_expr(ctx, reject)?;
    let ty = expr_scalar_kind(ctx.module, ctx.func, accept)?;
    let dst_ty = match ty {
        ScalarKind::Float => IrType::F32,
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => IrType::I32,
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
            return Err(LowerError::UnsupportedType(String::from("abstract select")));
        }
    };
    let dst = ctx.fb.alloc_vreg(dst_ty);
    ctx.fb.push(Op::Select {
        dst,
        cond,
        if_true: t,
        if_false: f,
    });
    Ok(dst)
}

fn lower_as(
    ctx: &mut LowerCtx<'_>,
    inner: Handle<Expression>,
    target: ScalarKind,
) -> Result<VReg, LowerError> {
    let src_k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    let v = lower_expr(ctx, inner)?;
    if src_k == target {
        return Ok(v);
    }
    let dst_ty = match target {
        ScalarKind::Float => IrType::F32,
        ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool => IrType::I32,
        ScalarKind::AbstractInt | ScalarKind::AbstractFloat => {
            return Err(LowerError::UnsupportedType(String::from(
                "abstract As target",
            )));
        }
    };
    let dst = ctx.fb.alloc_vreg(dst_ty);
    match (src_k, target) {
        (ScalarKind::Float, ScalarKind::Sint) => ctx.fb.push(Op::FtoiSatS { dst, src: v }),
        (ScalarKind::Float, ScalarKind::Uint) => ctx.fb.push(Op::FtoiSatU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Float) | (ScalarKind::Bool, ScalarKind::Float) => {
            ctx.fb.push(Op::ItofS { dst, src: v })
        }
        (ScalarKind::Uint, ScalarKind::Float) => ctx.fb.push(Op::ItofU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Uint) | (ScalarKind::Uint, ScalarKind::Sint) => {
            ctx.fb.push(Op::Copy { dst, src: v })
        }
        (ScalarKind::Bool, ScalarKind::Sint) | (ScalarKind::Bool, ScalarKind::Uint) => {
            ctx.fb.push(Op::Copy { dst, src: v })
        }
        (ScalarKind::Sint, ScalarKind::Bool) | (ScalarKind::Uint, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(Op::IconstI32 { dst: z, value: 0 });
            ctx.fb.push(Op::Ine {
                dst,
                lhs: v,
                rhs: z,
            });
        }
        (ScalarKind::Float, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::FconstF32 { dst: z, value: 0.0 });
            ctx.fb.push(Op::Fne {
                dst,
                lhs: v,
                rhs: z,
            });
        }
        _ => {
            return Err(LowerError::UnsupportedExpression(format!(
                "unsupported cast {src_k:?} -> {target:?}"
            )));
        }
    }
    Ok(dst)
}
