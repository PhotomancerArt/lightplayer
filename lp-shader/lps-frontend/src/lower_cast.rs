//! Naga `Expression::As` (casts) and scalar coercion for assignment.

use alloc::format;
use alloc::string::String;

use lpir::{IrType, LpirOp, VReg};
use naga::{Expression, Handle, ScalarKind, TypeInner};

use crate::lower_ctx::VRegVec;
use crate::lower_error::LowerError;
use crate::lower_expr::lower_expr_vec;
use crate::naga_util::expr_scalar_kind;

pub(crate) fn lower_as_vec(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    inner: Handle<Expression>,
    target: ScalarKind,
) -> Result<VRegVec, LowerError> {
    let inner_vs = lower_expr_vec(ctx, inner)?;
    let src_k = expr_scalar_kind(ctx.module, ctx.func, inner)?;
    if src_k == target {
        return Ok(inner_vs);
    }
    // GLSL 4.x: cast to a *scalar* numeric type from bvecN uses only the first component.
    // Naga types `Expression::As` as scalar; `lower_expr_vec(inner)` still has N lanes for bvecN.
    let src_regs: &[VReg] =
        if src_k == ScalarKind::Bool && target != ScalarKind::Bool && inner_vs.len() > 1 {
            &inner_vs[..1]
        } else {
            &inner_vs
        };
    let mut result = VRegVec::new();
    for &src in src_regs {
        let v = lower_as_scalar(ctx, src, src_k, target)?;
        result.push(v);
    }
    Ok(result)
}

pub(crate) fn lower_as_scalar(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    v: VReg,
    src_k: ScalarKind,
    target: ScalarKind,
) -> Result<VReg, LowerError> {
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
        (ScalarKind::Float, ScalarKind::Sint) => ctx.fb.push(LpirOp::FtoiSatS { dst, src: v }),
        (ScalarKind::Float, ScalarKind::Uint) => ctx.fb.push(LpirOp::FtoiSatU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Float) | (ScalarKind::Bool, ScalarKind::Float) => {
            ctx.fb.push(LpirOp::ItofS { dst, src: v })
        }
        (ScalarKind::Uint, ScalarKind::Float) => ctx.fb.push(LpirOp::ItofU { dst, src: v }),
        (ScalarKind::Sint, ScalarKind::Uint) | (ScalarKind::Uint, ScalarKind::Sint) => {
            ctx.fb.push(LpirOp::Copy { dst, src: v })
        }
        (ScalarKind::Bool, ScalarKind::Sint) | (ScalarKind::Bool, ScalarKind::Uint) => {
            ctx.fb.push(LpirOp::Copy { dst, src: v })
        }
        (ScalarKind::Sint, ScalarKind::Bool) | (ScalarKind::Uint, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst: z, value: 0 });
            ctx.fb.push(LpirOp::Ine {
                dst,
                lhs: v,
                rhs: z,
            });
        }
        (ScalarKind::Float, ScalarKind::Bool) => {
            let z = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 { dst: z, value: 0.0 });
            ctx.fb.push(LpirOp::Fne {
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

pub(crate) fn root_scalar_kind(inner: &TypeInner) -> Result<ScalarKind, LowerError> {
    match *inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } | TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        _ => Err(LowerError::Internal(String::from(
            "root_scalar_kind: expected scalar, vector, or matrix",
        ))),
    }
}
