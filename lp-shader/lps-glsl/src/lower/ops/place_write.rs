use alloc::format;

use crate::hir::{PlaceId, PlaceRoot};
use crate::{Diagnostic, Span};

use super::super::place::try_assign_place_direct;
use super::super::storage::{
    is_pointer_param, local_is_slot, param_pointer, store_global, store_local, store_value_to_addr,
};
use super::super::{LowerCtx, LowerValue};
use super::access::copy_value;
use super::place_project::assign_segments;
use super::place_read::root_value;

pub(in crate::lower) fn assign_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: PlaceId,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let place = ctx.arena.place(target);
    if place.segments.is_empty() {
        return assign_root(ctx, span, &place.root, value);
    }
    if try_assign_place_direct(ctx, span, target, &value)? {
        return Ok(());
    }
    let dst = root_value(ctx, span, &place.root)?;
    let dst = assign_segments(ctx, span, dst, &place.segments, value)?;
    write_root_back_if_memory_root(ctx, span, &place.root, &dst)
}

fn assign_root(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    match root {
        PlaceRoot::Local { local, .. } => store_local(ctx, span, *local, &value),
        PlaceRoot::Param { param, .. } => {
            if is_pointer_param(ctx, *param) {
                let addr = param_pointer(ctx, span, *param)?;
                store_value_to_addr(ctx, span, addr, &value)
            } else {
                let dst = ctx.params.get(*param).cloned().ok_or_else(|| {
                    Diagnostic::error(span, format!("parameter index {param} is out of range"))
                })?;
                copy_value(ctx, dst, value, span)
            }
        }
        PlaceRoot::Uniform { .. } => Err(Diagnostic::error(
            span,
            "assignment target cannot be a uniform",
        )),
        PlaceRoot::Global { byte_offset, .. } => store_global(ctx, span, *byte_offset, &value),
    }
}

fn write_root_back_if_memory_root(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    match root {
        PlaceRoot::Local { local, .. } if local_is_slot(ctx, *local) => {
            return store_local(ctx, span, *local, value);
        }
        PlaceRoot::Param { param, .. } if is_pointer_param(ctx, *param) => {
            let addr = param_pointer(ctx, span, *param)?;
            return store_value_to_addr(ctx, span, addr, value);
        }
        PlaceRoot::Global { byte_offset, .. } => {
            return store_global(ctx, span, *byte_offset, value);
        }
        _ => {}
    }
    Ok(())
}
