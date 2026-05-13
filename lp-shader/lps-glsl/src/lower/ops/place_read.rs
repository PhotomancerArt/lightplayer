use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::hir::{HirAssignTarget, HirExpr, PlaceRoot, PlaceSegment};
use crate::{Diagnostic, Span};

use super::super::storage::{is_pointer_param, load_value_from_addr, local_value, param_pointer};
use super::super::{LowerCtx, LowerValue, lower_expr};
use super::index::{lower_index, lower_index_field, lower_index_index};
use super::single_lane;

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

pub(super) fn read_assign_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
) -> Result<LowerValue, Diagnostic> {
    let place = &target.place;
    let value = root_value(ctx, span, &place.root)?;
    if place.segments.is_empty() {
        return Ok(value);
    }
    if let Some(lanes) = place.single_root_lane_path() {
        return read_root_lanes(span, value, &lanes, &place.ty);
    }
    match place.segments.as_slice() {
        [PlaceSegment::Index { index, ty }] => {
            let index = lower_expr(ctx, index)?;
            lower_index(ctx, span, value, index, ty)
        }
        [
            PlaceSegment::Index {
                index,
                ty: element_ty,
            },
            PlaceSegment::Field {
                lane_offset,
                lane_count,
                ty,
                ..
            },
        ] => {
            let array_lane_count = value.lanes.len();
            lower_index_field(
                ctx,
                span,
                value,
                index,
                element_ty,
                0,
                array_lane_count,
                *lane_offset,
                *lane_count,
                ty,
            )
        }
        [
            PlaceSegment::Field {
                lane_offset: array_lane_offset,
                lane_count: array_lane_count,
                ..
            },
            PlaceSegment::Index {
                index,
                ty: element_ty,
            },
            PlaceSegment::Field {
                lane_offset,
                lane_count,
                ty,
                ..
            },
        ] => lower_index_field(
            ctx,
            span,
            value,
            index,
            element_ty,
            *array_lane_offset,
            *array_lane_count,
            *lane_offset,
            *lane_count,
            ty,
        ),
        [
            PlaceSegment::Index { index: column, .. },
            PlaceSegment::Index { index: row, ty },
        ] if place.root_ty().is_matrix() => read_matrix_element(ctx, span, value, column, row, ty),
        [
            PlaceSegment::Index {
                index: outer_index,
                ty: outer_ty,
            },
            PlaceSegment::Index {
                index: inner_index,
                ty: inner_ty,
            },
        ] => lower_index_index(
            ctx,
            span,
            value,
            outer_index,
            outer_ty,
            inner_index,
            inner_ty,
        ),
        _ => Err(Diagnostic::error(
            span,
            "unsupported assignment target path",
        )),
    }
}

fn read_root_lanes(
    span: Span,
    value: LowerValue,
    lanes: &[usize],
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let mut out = Vec::new();
    for lane in lanes {
        let Some(value_lane) = value.lanes.get(*lane) else {
            return Err(Diagnostic::error(span, "lane read out of range"));
        };
        out.push(*value_lane);
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes: out,
    })
}

fn read_matrix_element(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    column: &HirExpr,
    row: &HirExpr,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let Some((cols, rows)) = value.ty.matrix_dims() else {
        return Err(Diagnostic::error(
            span,
            "matrix element base must be matrix",
        ));
    };
    let column = lower_expr(ctx, column)?;
    let column = single_lane(span, &column)?;
    let row = lower_expr(ctx, row)?;
    let row = single_lane(span, &row)?;
    let Some(mut selected) = value.lanes.first().copied() else {
        return Err(Diagnostic::error(span, "matrix element base has no lanes"));
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
            if col == 0 && row_index == 0 {
                continue;
            }
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
            let next = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::Select {
                dst: next,
                cond,
                if_true: value.lanes[dst_index],
                if_false: selected,
            });
            selected = next;
        }
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes: vec![selected],
    })
}
