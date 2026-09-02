use alloc::vec::Vec;

use lpir::VReg;
use lps_shared::LpsType;

use crate::hir::{PlaceRoot, PlaceSegment};
use crate::{Diagnostic, Span};

use super::super::storage::{LocalStorage, is_pointer_param, param_pointer};
use super::super::{Lanes, LowerCtx, lower_expr};
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
    pub(super) lanes: Lanes,
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
        PlaceRoot::Param { param } if is_pointer_param(ctx, *param) => {
            let base = param_pointer(ctx, span, *param)?;
            let ty = ctx
                .params
                .get(*param)
                .map(|value| value.ty.clone())
                .ok_or_else(|| {
                    Diagnostic::error(
                        span,
                        alloc::format!("parameter index {param} is out of range"),
                    )
                })?;
            Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: scalar_lane_offsets(&ty),
                ty,
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
        PlaceRoot::Uniform { name, byte_offset } => {
            let ty = ctx
                .uniforms
                .get(name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| {
                    Diagnostic::error(span, alloc::format!("unknown uniform `{name}`"))
                })?;
            Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: scalar_lane_offsets(&ty),
                ty,
                base: ctx.vmctx,
                static_offset: *byte_offset,
                dynamic_offset: None,
            }))
        }
        PlaceRoot::Global { name, byte_offset } => {
            let ty = ctx
                .globals
                .get(name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| {
                    Diagnostic::error(span, alloc::format!("unknown global `{name}`"))
                })?;
            Some(LoweredPlace::Memory(MemoryPlace {
                lane_offsets: scalar_lane_offsets(&ty),
                ty,
                base: ctx.vmctx,
                static_offset: *byte_offset,
                dynamic_offset: None,
            }))
        }
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
            member,
            lane_offset,
            lane_count,
            ..
        } => apply_field(
            place,
            span,
            *member,
            *lane_offset as usize,
            *lane_count as usize,
        ),
        PlaceSegment::Swizzle { .. } => {
            apply_swizzle(place, span, segment.swizzle_lanes().unwrap_or_default())
        }
        PlaceSegment::Index { index } => apply_index(ctx, span, place, *index),
    }
}

fn apply_field(
    place: LoweredPlace,
    span: Span,
    member: u16,
    lane_offset: usize,
    lane_count: usize,
) -> Result<Option<LoweredPlace>, Diagnostic> {
    let ty = crate::hir::field_type(place_ty_ref(&place), member)
        .cloned()
        .ok_or_else(|| Diagnostic::error(span, "field projection of a non-struct"))?;
    let ty = &ty;
    Ok(Some(match place {
        LoweredPlace::Flat(flat) => {
            let end = lane_offset + lane_count;
            let Some(lanes) = flat.lanes.get(lane_offset..end) else {
                return Err(Diagnostic::error(span, "field lane out of range"));
            };
            LoweredPlace::Flat(FlatPlace {
                ty: ty.clone(),
                lanes: Lanes::from_slice(lanes),
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
    lanes: &[u8],
) -> Result<Option<LoweredPlace>, Diagnostic> {
    let ty = crate::hir::swizzle_type(place_ty_ref(&place), lanes.len())
        .ok_or_else(|| Diagnostic::error(span, "swizzle of a non-vector"))?;
    let ty = &ty;
    Ok(Some(match place {
        LoweredPlace::Flat(flat) => {
            let projected = lanes
                .iter()
                .map(|lane| {
                    flat.lanes
                        .get(usize::from(*lane))
                        .copied()
                        .ok_or_else(|| Diagnostic::error(span, "swizzle lane out of range"))
                })
                .collect::<Result<Lanes, _>>()?;
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
                        .get(usize::from(*lane))
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
) -> Result<Option<LoweredPlace>, Diagnostic> {
    // Ask the place's type directly instead of building a shape table
    // (the old `TypeShape`), which cloned the whole type (for `Emitter[4]` that is the struct,
    // member names and all) per index — 200 such clones per meteor sim
    // compile, all transient.
    match place_ty_ref(&place) {
        LpsType::Array { element, len } => {
            let element = (**element).clone();
            let len = *len as usize;
            let stride =
                lps_shared::layout::array_stride(&element, lps_shared::LayoutRules::Std430);
            apply_array_index(ctx, span, place, index, &element, len, stride)
        }
        ty if ty.is_matrix() => {
            let column_ty = ty
                .matrix_column_type()
                .ok_or_else(|| Diagnostic::error(span, "index base must be matrix"))?;
            apply_flat_index(ctx, span, place, index, &column_ty)
        }
        ty => match crate::hir::scalar_base_type(ty) {
            Some(base) => apply_flat_index(ctx, span, place, index, &base),
            None => Ok(None),
        },
    }
}

fn apply_array_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    place: LoweredPlace,
    index: crate::hir::ExprId,
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
                ty: element.clone(),
                lanes: Lanes::from_slice(lanes),
            })))
        }
        LoweredPlace::Memory(memory) => {
            if let Some(index) = constant_index(ctx.arena.expr(index)) {
                if index >= len {
                    return Ok(None);
                }
                return Ok(Some(LoweredPlace::Memory(MemoryPlace {
                    lane_offsets: scalar_lane_offsets(element),
                    ty: element.clone(),
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
                lane_offsets: scalar_lane_offsets(element),
                ty: element.clone(),
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
    let place_ty = place_ty_ref(&place);
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
                lanes: Lanes::from_slice(lanes),
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

fn place_ty_ref(place: &LoweredPlace) -> &LpsType {
    match place {
        LoweredPlace::Flat(flat) => &flat.ty,
        LoweredPlace::Memory(memory) => &memory.ty,
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
