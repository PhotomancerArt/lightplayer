use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::body::{BinaryOp, IncDecOp};
use crate::hir::{HirAssignTarget, scalar_base_type, scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
use super::place_read::read_assign_target;
use super::place_write::assign_target;
use super::{lower_binary, single_lane};

pub(in crate::lower) fn lower_select(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    condition: LowerValue,
    accept: LowerValue,
    reject: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let cond = single_lane(span, &condition)?;
    if accept.lanes.len() != reject.lanes.len() {
        return Err(Diagnostic::error(span, "ternary arm lane count mismatch"));
    }
    let result_types = scalar_ir_types(result_ty)?;
    if result_types.len() != accept.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "ternary result lane count mismatch",
        ));
    }
    let mut lanes = Vec::new();
    for ((if_true, if_false), ty) in accept
        .lanes
        .iter()
        .zip(reject.lanes.iter())
        .zip(result_types.iter())
    {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Select {
            dst,
            cond,
            if_true: *if_true,
            if_false: *if_false,
        });
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn lower_inc_dec(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
    op: IncDecOp,
    prefix: bool,
) -> Result<LowerValue, Diagnostic> {
    let current = read_assign_target(ctx, span, target)?;
    let old = temp_copy(ctx, &current, span)?;
    let one = one_value(ctx, span, &current.ty)?;
    let binary_op = match op {
        IncDecOp::Increment => BinaryOp::Add,
        IncDecOp::Decrement => BinaryOp::Sub,
    };
    let updated = lower_binary(ctx, span, binary_op, old.clone(), one, &current.ty)?;
    assign_target(ctx, span, target, updated.clone())?;
    if prefix { Ok(updated) } else { Ok(old) }
}

pub(in crate::lower) fn temp_copy(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    span: Span,
) -> Result<LowerValue, Diagnostic> {
    let mut lanes = Vec::new();
    for (lane, ty) in value.lanes.iter().zip(scalar_ir_types(&value.ty)?) {
        let dst = ctx.fb.alloc_vreg(ty);
        ctx.fb.push(LpirOp::Copy { dst, src: *lane });
        lanes.push(dst);
    }
    if lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "temporary copy lane mismatch"));
    }
    Ok(LowerValue {
        ty: value.ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn one_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let base = scalar_base_type(ty).unwrap_or_else(|| ty.clone());
    let mut lanes = Vec::new();
    for _ in 0..scalar_lane_count(ty) {
        let lane = match base {
            LpsType::Float => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::FconstF32 { dst, value: 1.0 });
                dst
            }
            LpsType::Int => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
                dst
            }
            LpsType::UInt => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
                dst
            }
            _ => return Err(Diagnostic::error(span, "unsupported increment type")),
        };
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn copy_value(
    ctx: &mut LowerCtx<'_>,
    dst: LowerValue,
    src: LowerValue,
    span: Span,
) -> Result<(), Diagnostic> {
    if dst.lanes.len() != src.lanes.len() {
        return Err(Diagnostic::error(span, "copy lane count mismatch"));
    }
    for (dst, src) in dst.lanes.iter().zip(src.lanes.iter()) {
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src,
        });
    }
    Ok(())
}
