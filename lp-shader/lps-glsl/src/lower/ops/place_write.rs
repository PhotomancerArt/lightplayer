use alloc::format;

use lpir::{IrType, LpirOp};

use crate::hir::{HirAssignTarget, HirExpr, PlaceRoot, PlaceSegment};
use crate::{Diagnostic, Span};

use super::super::storage::{
    is_pointer_param, local_is_slot, param_pointer, store_local, store_local_lanes,
    store_value_to_addr,
};
use super::super::{LowerCtx, LowerValue, lower_expr};
use super::access::copy_value;
use super::index::{assign_index_field_target, assign_index_target};
use super::place_read::root_value;
use super::single_lane;

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
    match place.segments.as_slice() {
        [PlaceSegment::Index { index, ty }] => {
            let dst = root_value(ctx, span, &place.root)?;
            assign_index_target(ctx, span, dst.clone(), index, ty, value)?;
            write_root_back_if_memory_root(ctx, span, &place.root, &dst)
        }
        [
            PlaceSegment::Index {
                index,
                ty: element_ty,
            },
            PlaceSegment::Field {
                lane_offset,
                lane_count,
                ..
            },
        ] => {
            let dst = root_value(ctx, span, &place.root)?;
            assign_index_field_target(
                ctx,
                span,
                dst.clone(),
                index,
                element_ty,
                *lane_offset,
                *lane_count,
                value,
            )?;
            write_root_back_if_memory_root(ctx, span, &place.root, &dst)
        }
        [
            PlaceSegment::Index { index: column, .. },
            PlaceSegment::Index { index: row, .. },
        ] if place.root_ty().is_matrix() => {
            let dst = root_value(ctx, span, &place.root)?;
            assign_matrix_element(ctx, span, dst.clone(), column, row, value)?;
            write_root_back_if_memory_root(ctx, span, &place.root, &dst)
        }
        _ => Err(Diagnostic::error(
            span,
            "unsupported assignment target path",
        )),
    }
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

fn assign_matrix_element(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: LowerValue,
    column: &HirExpr,
    row: &HirExpr,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let Some((cols, rows)) = dst.ty.matrix_dims() else {
        return Err(Diagnostic::error(
            span,
            "matrix element base must be matrix",
        ));
    };
    let column = lower_expr(ctx, column)?;
    let column = single_lane(span, &column)?;
    let row = lower_expr(ctx, row)?;
    let row = single_lane(span, &row)?;
    let Some(src) = value.lanes.first().copied() else {
        return Err(Diagnostic::error(
            span,
            "matrix element assignment value has no lane",
        ));
    };
    for col in 0..cols {
        let col_constant = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: col_constant,
            value: col as i32,
        });
        let col_cond = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ieq {
            dst: col_cond,
            lhs: column,
            rhs: col_constant,
        });
        for row_index in 0..rows {
            let row_constant = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: row_constant,
                value: row_index as i32,
            });
            let row_cond = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Ieq {
                dst: row_cond,
                lhs: row,
                rhs: row_constant,
            });
            let cond = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Iand {
                dst: cond,
                lhs: col_cond,
                rhs: row_cond,
            });
            let dst_index = col * rows + row_index;
            let current = dst.lanes[dst_index];
            let selected = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Select {
                dst: selected,
                cond,
                if_true: src,
                if_false: current,
            });
            ctx.fb.push(LpirOp::Copy {
                dst: current,
                src: selected,
            });
        }
    }
    Ok(())
}
