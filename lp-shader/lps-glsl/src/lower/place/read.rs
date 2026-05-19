use alloc::vec::Vec;

use lpir::LpirOp;

use crate::hir::HirAssignTarget;
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
use super::path::{LoweredPlace, MemoryPlace, lower_place};

pub(in crate::lower) fn try_read_place_direct(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
) -> Result<Option<LowerValue>, Diagnostic> {
    let Some(place) = lower_place(ctx, span, &target.place.root, &target.place.segments)? else {
        return Ok(None);
    };
    Ok(Some(match place {
        LoweredPlace::Flat(flat) => LowerValue {
            ty: flat.ty,
            lanes: flat.lanes,
        },
        LoweredPlace::Memory(memory) => load_memory_place(ctx, memory)?,
    }))
}

fn load_memory_place(
    ctx: &mut LowerCtx<'_>,
    memory: MemoryPlace,
) -> Result<LowerValue, Diagnostic> {
    let ir_types = crate::hir::scalar_ir_types(&memory.ty)?;
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
    let mut lanes = Vec::new();
    if ir_types.len() != memory.lane_offsets.len() {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            "memory read lane mismatch",
        ));
    }
    for (offset, ir_ty) in memory.lane_offsets.iter().zip(ir_types.iter()) {
        let dst = ctx.fb.alloc_vreg(*ir_ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base,
            offset: memory.static_offset.saturating_add(*offset),
        });
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: memory.ty,
        lanes,
    })
}
