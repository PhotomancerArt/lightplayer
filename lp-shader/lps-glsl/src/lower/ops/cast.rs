use alloc::format;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, VReg};
use lps_shared::LpsType;

use crate::hir::{scalar_base_type, scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};

pub(in crate::lower) fn lower_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    target_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let src_base = scalar_base_type(&value.ty).ok_or_else(|| {
        Diagnostic::error(span, format!("unsupported cast source {:?}", value.ty))
    })?;
    let dst_base = scalar_base_type(target_ty)
        .ok_or_else(|| Diagnostic::error(span, format!("unsupported cast target {target_ty:?}")))?;
    if value.lanes.len() != scalar_lane_count(target_ty) {
        return Err(Diagnostic::error(span, "cast lane count mismatch"));
    }
    let dst_types = scalar_ir_types(target_ty)?;
    let mut lanes = Vec::new();
    for (src, dst_ty) in value.lanes.iter().zip(dst_types.iter()) {
        let dst = lower_scalar_cast(ctx, span, *src, &src_base, &dst_base, *dst_ty)?;
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: target_ty.clone(),
        lanes,
    })
}

fn lower_scalar_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    src: VReg,
    src_ty: &LpsType,
    dst_ty: &LpsType,
    dst_ir_ty: IrType,
) -> Result<VReg, Diagnostic> {
    let dst = ctx.fb.alloc_vreg(dst_ir_ty);
    match (src_ty, dst_ty) {
        (LpsType::Float, LpsType::Float)
        | (LpsType::Int, LpsType::Int)
        | (LpsType::UInt, LpsType::UInt)
        | (LpsType::Bool, LpsType::Bool)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int) => ctx.fb.push(LpirOp::Copy { dst, src }),
        (LpsType::Int, LpsType::Float) | (LpsType::Bool, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofS { dst, src });
        }
        (LpsType::UInt, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofU { dst, src });
        }
        (LpsType::Float, LpsType::Int) => {
            ctx.fb.push(LpirOp::FtoiSatS { dst, src });
        }
        (LpsType::Float, LpsType::UInt) => {
            ctx.fb.push(LpirOp::FtoiSatU { dst, src });
        }
        (LpsType::Int | LpsType::UInt, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: zero,
                value: 0,
            });
            ctx.fb.push(LpirOp::Ine {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        (LpsType::Float, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 {
                dst: zero,
                value: 0.0,
            });
            ctx.fb.push(LpirOp::Fne {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported scalar cast {src_ty:?} to {dst_ty:?}"),
            ));
        }
    }
    Ok(dst)
}
