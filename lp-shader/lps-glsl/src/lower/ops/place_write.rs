use alloc::format;

use lpir::LpirOp;

use crate::hir::{HirAssignTarget, PlaceRoot};
use crate::{Diagnostic, Span};

use super::super::storage::{
    is_pointer_param, local_is_slot, param_pointer, store_local, store_local_lanes,
    store_value_to_addr,
};
use super::super::{LowerCtx, LowerValue};
use super::access::copy_value;
use super::place_project::assign_segments;
use super::place_read::root_value;

pub(in crate::lower) fn assign_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let place = &target.place;
    if place.segments.is_empty() {
        return assign_root(ctx, span, &place.root, value);
    }
    if let Some(lanes) = place.single_root_lane_path() {
        return assign_root_lanes(ctx, span, &place.root, &lanes, value);
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
    }
}

fn assign_root_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    lanes: &[usize],
    value: LowerValue,
) -> Result<(), Diagnostic> {
    if lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "lane assignment width mismatch"));
    }
    match root {
        PlaceRoot::Local { local, .. } => store_local_lanes(ctx, span, *local, lanes, &value),
        PlaceRoot::Param { param, .. } => {
            if is_pointer_param(ctx, *param) {
                let addr = param_pointer(ctx, span, *param)?;
                for (dst_lane, src_lane) in lanes.iter().zip(value.lanes.iter()) {
                    ctx.fb.push(LpirOp::Store {
                        base: addr,
                        offset: (*dst_lane as u32) * 4,
                        value: *src_lane,
                    });
                }
                Ok(())
            } else {
                let dst = ctx.params.get(*param).cloned().ok_or_else(|| {
                    Diagnostic::error(span, format!("parameter index {param} is out of range"))
                })?;
                copy_lanes(ctx, span, &dst, lanes, &value)
            }
        }
        PlaceRoot::Uniform { .. } => Err(Diagnostic::error(
            span,
            "assignment target cannot be a uniform",
        )),
    }
}

fn copy_lanes(
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
        _ => {}
    }
    Ok(())
}
