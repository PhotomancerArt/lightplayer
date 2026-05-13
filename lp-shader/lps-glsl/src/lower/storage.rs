use alloc::format;
use alloc::vec::Vec;

use lpir::{FunctionBuilder, IrType, LpirOp, SlotId, VReg};
use lps_shared::{LpsType, ParamQualifier};

use crate::hir::{scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::{LowerCtx, LowerValue};

#[derive(Debug, Clone)]
pub(super) enum LocalStorage {
    Flat(LowerValue),
    Slot { ty: LpsType, addr: VReg },
}

pub(super) fn flat_value_byte_size(ty: &LpsType) -> u32 {
    scalar_lane_count(ty) as u32 * 4
}

pub(super) fn local_storage(
    fb: &mut FunctionBuilder,
    ty: LpsType,
) -> Result<LocalStorage, Diagnostic> {
    if should_slot_back_local(&ty) {
        let (_slot, addr) = alloc_slot_addr_in(fb, flat_value_byte_size(&ty), IrType::Pointer);
        return Ok(LocalStorage::Slot { ty, addr });
    }

    let mut lanes = Vec::new();
    for ir_ty in scalar_ir_types(&ty)? {
        lanes.push(fb.alloc_vreg(ir_ty));
    }
    Ok(LocalStorage::Flat(LowerValue { ty, lanes }))
}

pub(super) fn local_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    local: usize,
) -> Result<LowerValue, Diagnostic> {
    let storage =
        ctx.locals.get(local).cloned().ok_or_else(|| {
            Diagnostic::error(span, format!("local index {local} is out of range"))
        })?;
    match storage {
        LocalStorage::Flat(value) => Ok(value),
        LocalStorage::Slot { ty, addr, .. } => load_value_from_addr(ctx, span, addr, &ty),
    }
}

pub(super) fn store_local(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    local: usize,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    let storage =
        ctx.locals.get(local).cloned().ok_or_else(|| {
            Diagnostic::error(span, format!("local index {local} is out of range"))
        })?;
    match storage {
        LocalStorage::Flat(dst) => copy_lanes_to_value(ctx, span, &dst, value),
        LocalStorage::Slot { addr, .. } => store_value_to_addr(ctx, span, addr, value),
    }
}

pub(super) fn store_local_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    local: usize,
    lanes: &[usize],
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "lane assignment width mismatch"));
    }
    let storage =
        ctx.locals.get(local).cloned().ok_or_else(|| {
            Diagnostic::error(span, format!("local index {local} is out of range"))
        })?;
    match storage {
        LocalStorage::Flat(dst) => copy_selected_lanes(ctx, span, &dst, lanes, value),
        LocalStorage::Slot { ty, addr, .. } => {
            let dst = load_value_from_addr(ctx, span, addr, &ty)?;
            copy_selected_lanes(ctx, span, &dst, lanes, value)?;
            store_value_to_addr(ctx, span, addr, &dst)
        }
    }
}

pub(super) fn local_is_slot(ctx: &LowerCtx<'_>, local: usize) -> bool {
    matches!(ctx.locals.get(local), Some(LocalStorage::Slot { .. }))
}

fn should_slot_back_local(ty: &LpsType) -> bool {
    matches!(ty, LpsType::Array { .. })
}

pub(super) fn alloc_slot_addr(
    ctx: &mut LowerCtx<'_>,
    byte_size: u32,
    addr_ty: IrType,
) -> (SlotId, VReg) {
    alloc_slot_addr_in(&mut ctx.fb, byte_size, addr_ty)
}

fn alloc_slot_addr_in(fb: &mut FunctionBuilder, byte_size: u32, addr_ty: IrType) -> (SlotId, VReg) {
    let slot = fb.alloc_slot(byte_size);
    let addr = fb.alloc_vreg(addr_ty);
    fb.push(LpirOp::SlotAddr { dst: addr, slot });
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
            base: ctx.vmctx,
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

pub(super) fn lower_global_load(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    byte_offset: u32,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    lower_uniform_load(ctx, span, byte_offset, ty)
}

pub(super) fn store_global(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    byte_offset: u32,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if value.lanes.len() != scalar_lane_count(&value.ty) {
        return Err(Diagnostic::error(span, "global store lane count mismatch"));
    }
    for (i, lane) in value.lanes.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: ctx.vmctx,
            offset: byte_offset.saturating_add((i as u32).saturating_mul(4)),
            value: *lane,
        });
    }
    Ok(())
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

fn copy_lanes_to_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: &LowerValue,
    src: &LowerValue,
) -> Result<(), Diagnostic> {
    if dst.lanes.len() != src.lanes.len() {
        return Err(Diagnostic::error(span, "copy lane count mismatch"));
    }
    copy_selected_lanes(
        ctx,
        span,
        dst,
        &(0..dst.lanes.len()).collect::<Vec<_>>(),
        src,
    )
}

fn copy_selected_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: &LowerValue,
    lanes: &[usize],
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    for (dst_lane, src_lane) in lanes.iter().zip(value.lanes.iter()) {
        let Some(dst) = dst.lanes.get(*dst_lane) else {
            return Err(Diagnostic::error(span, "assignment lane out of range"));
        };
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src_lane,
        });
    }
    Ok(())
}
