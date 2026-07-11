use alloc::vec::Vec;

use lpir::VReg;
use lps_shared::LpsType;

use crate::hir::{PlaceRoot, PlaceSegment, TypeShape};
use crate::{Diagnostic, Span};

use super::super::storage::{LocalStorage, is_pointer_param, param_pointer};
use super::super::{LowerCtx, lower_expr};
use super::dynamic;
use super::layout::{constant_index, scalar_lane_offsets};

#[derive(Clone)]
pub(super) enum LoweredPlace {
    Flat(FlatPlace),
    Memory(MemoryPlace),
}

#[derive(Clone)]
pub(super) struct FlatPlace {
    pub(super) ty: LpsType,
    pub(super) lanes: Vec<VReg>,
}

#[derive(Clone)]
pub(super) struct MemoryPlace {
    pub(super) ty: LpsType,
    pub(super) base: VReg,
    pub(super) static_offset: u32,
    pub(super) dynamic_offset: Option<VReg>,
    pub(super) lane_offsets: Vec<u32>,
}

pub(super) fn lower_place(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    segments: &[PlaceSegment],
) -> Result<Option<LoweredPlace>, Diagnostic> {
    let Some(mut place) = root_place(ctx, span, root)? else {
        return Ok(None);
    };
    for segment in segments {
        let Some(next) = apply_segment(ctx, span, place, segment)? else {
            return Ok(None);
        };
        place = next;
    }
    Ok(Some(place))
}

fn root_place(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    Ok(match root {
        PlaceRoot::Local { local, .. } => {
            match ctx.locals.get(*local).cloned().ok_or_else(|| {
                Diagnostic::error(span, alloc::format!("local index {local} is out of range"))
            })? {
                LocalStorage::Flat(value) => Some(LoweredPlace::Flat(FlatPlace {
                    ty: value.ty,
                    lanes: value.lanes,
                })),
                LocalStorage::Slot { ty, addr } => Some(LoweredPlace::Memory(MemoryPlace {
                    lane_offsets: scalar_lane_offsets(&ty),
                    ty,
                    base: addr,
                    static_offset: 0,
                    dynamic_offset: None,
                })),
            }
        }
        PlaceRoot::Param { param, ty } if is_pointer_param(ctx, *param) => {
            let base = param_pointer(ctx, span, *param)?;
            Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: scalar_lane_offsets(ty),
                ty: ty.clone(),
                base,
                static_offset: 0,
                dynamic_offset: None,
            }))
        }
        PlaceRoot::Param { param, .. } => {
            let value = ctx.params.get(*param).cloned().ok_or_else(|| {
                Diagnostic::error(
                    span,
                    alloc::format!("parameter index {param} is out of range"),
                )
            })?;
            Some(LoweredPlace::Flat(FlatPlace {
                ty: value.ty,
                lanes: value.lanes,
            }))
        }
        PlaceRoot::Uniform {
            byte_offset, ty, ..
        }
        | PlaceRoot::Global {
            byte_offset, ty, ..
        } => Some(LoweredPlace::Memory(MemoryPlace {
            lane_offsets: scalar_lane_offsets(ty),
            ty: ty.clone(),
            base: ctx.vmctx,
            static_offset: *byte_offset,
            dynamic_offset: None,
        })),
    })
}

fn apply_segment(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    place: LoweredPlace,
    segment: &PlaceSegment,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    match segment {
        PlaceSegment::Field {
            lane_offset,
            lane_count,
            byte_offset,
            ty,
            ..
        } => apply_field(place, span, *lane_offset, *lane_count, *byte_offset, ty),
        PlaceSegment::Swizzle { lanes, ty, .. } => apply_swizzle(place, span, lanes, ty),
        PlaceSegment::Index { index, ty } => apply_index(ctx, span, place, *index, ty),
    }
}

fn apply_field(
    place: LoweredPlace,
    span: Span,
    lane_offset: usize,
    lane_count: usize,
    _byte_offset: usize,
    ty: &LpsType,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    Ok(Some(match place {
        LoweredPlace::Flat(flat) => {
            let end = lane_offset + lane_count;
            let Some(lanes) = flat.lanes.get(lane_offset..end) else {
                return Err(Diagnostic::error(span, "field lane out of range"));
            };
            LoweredPlace::Flat(FlatPlace {
                ty: ty.clone(),
                lanes: lanes.to_vec(),
            })
        }
        LoweredPlace::Memory(memory) => LoweredPlace::Memory(MemoryPlace {
            lane_offsets: slice_lane_offsets(span, &memory.lane_offsets, lane_offset, lane_count)?,
            ty: ty.clone(),
            base: memory.base,
            static_offset: memory.static_offset,
            dynamic_offset: memory.dynamic_offset,
        }),
    }))
}

fn apply_swizzle(
    place: LoweredPlace,
    span: Span,
    lanes: &[usize],
    ty: &LpsType,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    Ok(Some(match place {
        LoweredPlace::Flat(flat) => {
            let projected = lanes
                .iter()
                .map(|lane| {
                    flat.lanes
                        .get(*lane)
                        .copied()
                        .ok_or_else(|| Diagnostic::error(span, "swizzle lane out of range"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            LoweredPlace::Flat(FlatPlace {
                ty: ty.clone(),
                lanes: projected,
            })
        }
        LoweredPlace::Memory(memory) => {
            let lane_offsets = lanes
                .iter()
                .map(|lane| {
                    memory
                        .lane_offsets
                        .get(*lane)
                        .copied()
                        .ok_or_else(|| Diagnostic::error(span, "swizzle lane out of range"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            LoweredPlace::Memory(MemoryPlace {
                lane_offsets,
                ty: ty.clone(),
                base: memory.base,
                static_offset: memory.static_offset,
                dynamic_offset: memory.dynamic_offset,
            })
        }
    }))
}

fn apply_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    place: LoweredPlace,
    index: crate::hir::ExprId,
    ty: &LpsType,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    let place_ty = place_ty(&place);
    let shape = TypeShape::new(&place_ty);
    if let Some((element, len, stride)) = shape.array_element() {
        return apply_array_index(ctx, span, place, index, ty, element, len as usize, stride);
    }
    if let Some(column_ty) = shape.matrix_column() {
        return apply_flat_index(ctx, span, place, index, column_ty);
    }
    if let Some(base) = crate::hir::scalar_base_type(&place_ty) {
        return apply_flat_index(ctx, span, place, index, &base);
    }
    Ok(None)
}

fn apply_array_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    place: LoweredPlace,
    index: crate::hir::ExprId,
    ty: &LpsType,
    element: &LpsType,
    len: usize,
    stride: usize,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    match place {
        LoweredPlace::Flat(flat) => {
            let Some(index) = constant_index(ctx.arena.expr(index)) else {
                return Ok(None);
            };
            if index >= len {
                return Ok(None);
            }
            let width = crate::hir::scalar_lane_count(element);
            let start = index * width;
            let end = start + width;
            let Some(lanes) = flat.lanes.get(start..end) else {
                return Err(Diagnostic::error(span, "array index lane out of range"));
            };
            Ok(Some(LoweredPlace::Flat(FlatPlace {
                ty: ty.clone(),
                lanes: lanes.to_vec(),
            })))
        }
        LoweredPlace::Memory(memory) => {
            if let Some(index) = constant_index(ctx.arena.expr(index)) {
                if index >= len {
                    return Ok(None);
                }
                return Ok(Some(LoweredPlace::Memory(MemoryPlace {
                    lane_offsets: scalar_lane_offsets(ty),
                    ty: ty.clone(),
                    base: memory.base,
                    static_offset: memory
                        .static_offset
                        .saturating_add(index.saturating_mul(stride) as u32),
                    dynamic_offset: memory.dynamic_offset,
                })));
            }
            let index = lower_expr(ctx, index)?;
            let index = dynamic::clamp_index(ctx, span, index, len)?;
            let offset = dynamic::scale_index(ctx, index, stride);
            Ok(Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: scalar_lane_offsets(ty),
                ty: ty.clone(),
                base: memory.base,
                static_offset: memory.static_offset,
                dynamic_offset: Some(dynamic::add_offsets(ctx, memory.dynamic_offset, offset)),
            })))
        }
    }
}

fn apply_flat_index(
    ctx: &mut LowerCtx<'_>,
    _span: Span,
    place: LoweredPlace,
    index: crate::hir::ExprId,
    ty: &LpsType,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    let Some(index) = constant_index(ctx.arena.expr(index)) else {
        return Ok(None);
    };
    let width = crate::hir::scalar_lane_count(ty);
    let place_ty = place_ty(&place);
    let source_width = if place_ty.is_matrix() || place_ty.is_array() {
        width
    } else {
        1
    };
    let start = index * source_width;
    let end = start + width;
    Ok(match place {
        LoweredPlace::Flat(flat) => {
            let source_count = flat.lanes.len() / source_width;
            if index >= source_count {
                return Ok(None);
            }
            let Some(lanes) = flat.lanes.get(start..end) else {
                return Ok(None);
            };
            Some(LoweredPlace::Flat(FlatPlace {
                ty: ty.clone(),
                lanes: lanes.to_vec(),
            }))
        }
        LoweredPlace::Memory(memory) => {
            let source_count = memory.lane_offsets.len() / source_width;
            if index >= source_count {
                return Ok(None);
            }
            let Some(lane_offsets) = memory.lane_offsets.get(start..end) else {
                return Ok(None);
            };
            Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: lane_offsets.to_vec(),
                ty: ty.clone(),
                base: memory.base,
                static_offset: memory.static_offset,
                dynamic_offset: memory.dynamic_offset,
            }))
        }
    })
}

fn place_ty(place: &LoweredPlace) -> LpsType {
    match place {
        LoweredPlace::Flat(flat) => flat.ty.clone(),
        LoweredPlace::Memory(memory) => memory.ty.clone(),
    }
}

fn slice_lane_offsets(
    span: Span,
    lane_offsets: &[u32],
    lane_offset: usize,
    lane_count: usize,
) -> Result<Vec<u32>, Diagnostic> {
    let end = lane_offset + lane_count;
    let Some(offsets) = lane_offsets.get(lane_offset..end) else {
        return Err(Diagnostic::error(span, "field lane out of range"));
    };
    Ok(offsets.to_vec())
}
