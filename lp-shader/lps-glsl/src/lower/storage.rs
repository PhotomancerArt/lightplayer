use alloc::format;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp, SlotId, VMCTX_VREG, VReg};
use lps_shared::{LpsType, ParamQualifier};

use crate::hir::{scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::{LowerCtx, LowerValue};

pub(super) fn flat_value_byte_size(ty: &LpsType) -> u32 {
    scalar_lane_count(ty) as u32 * 4
}

pub(super) fn alloc_slot_addr(
    ctx: &mut LowerCtx<'_>,
    byte_size: u32,
    addr_ty: IrType,
) -> (SlotId, VReg) {
    let slot = ctx.fb.alloc_slot(byte_size);
    let addr = ctx.fb.alloc_vreg(addr_ty);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
    (slot, addr)
}

pub(super) fn lower_uniform_load(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    byte_offset: u32,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let ir_types = scalar_ir_types(ty)?;
    let mut lanes = Vec::new();
    for (i, ir_ty) in ir_types.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ir_ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: VMCTX_VREG,
            offset: byte_offset.saturating_add((i as u32).saturating_mul(4)),
        });
        lanes.push(dst);
    }
    if lanes.len() != scalar_lane_count(ty) {
        return Err(Diagnostic::error(span, "uniform lane count mismatch"));
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes,
    })
}

pub(super) fn is_pointer_param(ctx: &LowerCtx<'_>, param: usize) -> bool {
    ctx.param_qualifiers
        .get(param)
        .is_some_and(|q| matches!(q, ParamQualifier::Out | ParamQualifier::InOut))
}

pub(super) fn param_pointer(
    ctx: &LowerCtx<'_>,
    span: Span,
    param: usize,
) -> Result<VReg, Diagnostic> {
    let value = ctx.params.get(param).ok_or_else(|| {
        Diagnostic::error(span, format!("parameter index {param} is out of range"))
    })?;
    value
        .lanes
        .first()
        .copied()
        .ok_or_else(|| Diagnostic::error(span, "pointer parameter has no address lane"))
}

pub(super) fn load_value_from_addr(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    addr: VReg,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let ir_types = scalar_ir_types(ty)?;
    let mut lanes = Vec::new();
    for (i, ir_ty) in ir_types.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ir_ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: addr,
            offset: i as u32 * 4,
        });
        lanes.push(dst);
    }
    if lanes.len() != scalar_lane_count(ty) {
        return Err(Diagnostic::error(span, "load lane count mismatch"));
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes,
    })
}

pub(super) fn store_value_to_addr(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    addr: VReg,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if value.lanes.len() != scalar_lane_count(&value.ty) {
        return Err(Diagnostic::error(span, "store lane count mismatch"));
    }
    for (i, lane) in value.lanes.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: addr,
            offset: i as u32 * 4,
            value: *lane,
        });
    }
    Ok(())
}
