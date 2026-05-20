use lpir::LpirOp;

use crate::hir::PlaceId;
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
use super::path::{LoweredPlace, MemoryPlace, lower_place};

pub(in crate::lower) fn try_assign_place_direct(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: PlaceId,
    value: &LowerValue,
) -> Result<bool, Diagnostic> {
    let target = ctx.arena.place(target);
    let Some(place) = lower_place(ctx, span, &target.root, &target.segments)? else {
        return Ok(false);
    };
    match place {
        LoweredPlace::Flat(flat) => {
            if flat.lanes.len() != value.lanes.len() {
                return Err(Diagnostic::error(span, "place assignment lane mismatch"));
            }
            for (dst, src) in flat.lanes.iter().zip(value.lanes.iter()) {
                ctx.fb.push(LpirOp::Copy {
                    dst: *dst,
                    src: *src,
                });
            }
            Ok(true)
        }
        LoweredPlace::Memory(memory) => {
            store_memory_place(ctx, span, memory, value)?;
            Ok(true)
        }
    }
}

pub(super) fn store_memory_place(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    memory: MemoryPlace,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if crate::hir::scalar_lane_count(&memory.ty) != value.lanes.len() {
        return Err(Diagnostic::error(span, "memory assignment lane mismatch"));
    }
    if memory.lane_offsets.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "memory assignment offset mismatch"));
    }
    let base = if let Some(dynamic_offset) = memory.dynamic_offset {
        let base = ctx.fb.alloc_vreg(lpir::IrType::I32);
        ctx.fb.push(LpirOp::Iadd {
            dst: base,
            lhs: memory.base,
            rhs: dynamic_offset,
        });
        base
    } else {
        memory.base
    };
    for (offset, lane) in memory.lane_offsets.iter().zip(value.lanes.iter()) {
        ctx.fb.push(LpirOp::Store {
            base,
            offset: memory.static_offset.saturating_add(*offset),
            value: *lane,
        });
    }
    Ok(())
}
