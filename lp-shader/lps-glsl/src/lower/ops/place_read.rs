use alloc::format;

use crate::hir::{HirAssignTarget, PlaceRoot};
use crate::{Diagnostic, Span};

use super::super::storage::{is_pointer_param, load_value_from_addr, local_value, param_pointer};
use super::super::{LowerCtx, LowerValue};
use super::place_project::read_segments;

pub(super) fn root_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
) -> Result<LowerValue, Diagnostic> {
    match root {
        PlaceRoot::Local { local, .. } => local_value(ctx, span, *local),
        PlaceRoot::Param { param, .. } => {
            if is_pointer_param(ctx, *param) {
                let addr = param_pointer(ctx, span, *param)?;
                let param_ty = ctx.params[*param].ty.clone();
                load_value_from_addr(ctx, span, addr, &param_ty)
            } else {
                ctx.params.get(*param).cloned().ok_or_else(|| {
                    Diagnostic::error(span, format!("parameter index {param} is out of range"))
                })
            }
        }
        PlaceRoot::Uniform { .. } => Err(Diagnostic::error(
            span,
            "assignment target cannot be a uniform",
        )),
    }
}

pub(in crate::lower) fn read_assign_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
) -> Result<LowerValue, Diagnostic> {
    let place = &target.place;
    let value = root_value(ctx, span, &place.root)?;
    if place.segments.is_empty() {
        return Ok(value);
    }
    read_segments(ctx, span, value, &place.segments)
}
