//! Naga unary operators → LPIR.

use alloc::string::String;

use lpir::{IrType, Op, VReg};
use naga::{Expression, Handle, ScalarKind, UnaryOperator};

use crate::lower_ctx::VRegVec;
use crate::lower_error::LowerError;
use crate::lower_expr::lower_expr_vec;
use crate::naga_util::expr_scalar_kind;

pub(crate) fn lower_unary_vec(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: UnaryOperator,
    inner: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    let mut result = VRegVec::new();
    for &src in &inner_vs {
        let v = lower_unary_scalar(ctx, op, src, k)?;
        result.push(v);
    }
    Ok(result)
}

fn lower_unary_scalar(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    op: UnaryOperator,
    src: VReg,
    k: ScalarKind,
) -> Result<VReg, LowerError> {
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
