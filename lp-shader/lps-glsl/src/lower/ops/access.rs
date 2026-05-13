use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::body::{BinaryOp, IncDecOp};
use crate::hir::{
    HirAssignTarget, HirExpr, PlaceRoot, PlaceSegment, scalar_base_type, scalar_ir_types,
    scalar_lane_count,
};
use crate::{Diagnostic, Span};

use super::super::{
    LowerCtx, LowerValue, is_pointer_param, load_value_from_addr, lower_expr, param_pointer,
    store_value_to_addr,
};
use super::{lower_binary, single_lane};

pub(in crate::lower) fn lower_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    base: LowerValue,
    index: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let index = single_lane(span, &index)?;
    let result_width = scalar_lane_count(result_ty);
    if result_width == 0 {
        return Err(Diagnostic::error(span, "index result has no lanes"));
    }
    let result_ir_types = scalar_ir_types(result_ty)?;
    let source_width = if base.ty.is_matrix() || base.ty.is_array() {
        result_width
    } else {
        1
    };
    let source_count = base.lanes.len() / source_width;
    let mut lanes = Vec::new();
    for component in 0..result_width {
        let Some(mut selected) = base.lanes.get(component).copied() else {
            return Err(Diagnostic::error(span, "index base has no lanes"));
        };
        let result_ir_ty = result_ir_types
            .get(component)
            .copied()
            .ok_or_else(|| Diagnostic::error(span, "index result has no type"))?;
        for lane_index in 1..source_count {
            let Some(lane) = base.lanes.get(lane_index * source_width + component) else {
                return Err(Diagnostic::error(span, "index base lane out of range"));
            };
            let constant = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: constant,
                value: lane_index as i32,
            });
            let cond = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Ieq {
                dst: cond,
                lhs: index,
                rhs: constant,
            });
            let dst = ctx.fb.alloc_vreg(result_ir_ty);
            ctx.fb.push(LpirOp::Select {
                dst,
                cond,
                if_true: *lane,
                if_false: selected,
            });
            selected = dst;
        }
        lanes.push(selected);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

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
            write_root_back_if_pointer_param(ctx, span, &place.root, &dst)
        }
        [
            PlaceSegment::Index { index: column, .. },
            PlaceSegment::Index { index: row, .. },
        ] if place.root_ty().is_matrix() => {
            let dst = root_value(ctx, span, &place.root)?;
            assign_matrix_element(ctx, span, dst.clone(), column, row, value)?;
            write_root_back_if_pointer_param(ctx, span, &place.root, &dst)
        }
        _ => Err(Diagnostic::error(
            span,
            "unsupported assignment target path",
        )),
    }
}

fn root_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
) -> Result<LowerValue, Diagnostic> {
    match root {
        PlaceRoot::Local { local, .. } => {
            ctx.locals.get(*local).cloned().ok_or_else(|| {
                Diagnostic::error(span, format!("local index {local} is out of range"))
            })
        }
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

fn assign_root(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    match root {
        PlaceRoot::Local { local, .. } => {
            let dst = ctx.locals.get(*local).cloned().ok_or_else(|| {
                Diagnostic::error(span, format!("local index {local} is out of range"))
            })?;
            copy_value(ctx, dst, value, span)
        }
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
        PlaceRoot::Local { local, .. } => {
            let dst = ctx.locals.get(*local).cloned().ok_or_else(|| {
                Diagnostic::error(span, format!("local index {local} is out of range"))
            })?;
            copy_lanes(ctx, span, &dst, lanes, &value)
        }
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

fn write_root_back_if_pointer_param(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    root: &PlaceRoot,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if let PlaceRoot::Param { param, .. } = root
        && is_pointer_param(ctx, *param)
    {
        let addr = param_pointer(ctx, span, *param)?;
        return store_value_to_addr(ctx, span, addr, value);
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

fn assign_index_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: LowerValue,
    index: &HirExpr,
    ty: &LpsType,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let index = lower_expr(ctx, index)?;
    let index = single_lane(span, &index)?;
    let width = scalar_lane_count(ty);
    if width == 0 || width != value.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "index assignment value lane mismatch",
        ));
    }
    let lane_types = scalar_ir_types(&value.ty)?;
    let count = dst.lanes.len() / width;
    for lane_index in 0..count {
        let constant = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: constant,
            value: lane_index as i32,
        });
        let cond = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ieq {
            dst: cond,
            lhs: index,
            rhs: constant,
        });
        for component in 0..width {
            let dst_index = lane_index * width + component;
            let current = dst.lanes[dst_index];
            let selected = ctx.fb.alloc_vreg(lane_types[component]);
            ctx.fb.push(LpirOp::Select {
                dst: selected,
                cond,
                if_true: value.lanes[component],
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

pub(in crate::lower) fn lower_select(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    condition: LowerValue,
    accept: LowerValue,
    reject: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let cond = single_lane(span, &condition)?;
    if accept.lanes.len() != reject.lanes.len() {
        return Err(Diagnostic::error(span, "ternary arm lane count mismatch"));
    }
    let result_types = scalar_ir_types(result_ty)?;
    if result_types.len() != accept.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "ternary result lane count mismatch",
        ));
    }
    let mut lanes = Vec::new();
    for ((if_true, if_false), ty) in accept
        .lanes
        .iter()
        .zip(reject.lanes.iter())
        .zip(result_types.iter())
    {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Select {
            dst,
            cond,
            if_true: *if_true,
            if_false: *if_false,
        });
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn lower_inc_dec(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    target: &HirAssignTarget,
    op: IncDecOp,
    prefix: bool,
) -> Result<LowerValue, Diagnostic> {
    let current = read_assign_target(ctx, span, target)?;
    let old = temp_copy(ctx, &current, span)?;
    let one = one_value(ctx, span, &current.ty)?;
    let binary_op = match op {
        IncDecOp::Increment => BinaryOp::Add,
        IncDecOp::Decrement => BinaryOp::Sub,
    };
    let updated = lower_binary(ctx, span, binary_op, old.clone(), one, &current.ty)?;
    assign_target(ctx, span, target, updated.clone())?;
    if prefix { Ok(updated) } else { Ok(old) }
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
    if let Some(lanes) = place.single_root_lane_path() {
        return read_root_lanes(span, value, &lanes, &place.ty);
    }
    match place.segments.as_slice() {
        [PlaceSegment::Index { index, ty }] => {
            let index = lower_expr(ctx, index)?;
            lower_index(ctx, span, value, index, ty)
        }
        [
            PlaceSegment::Index { index: column, .. },
            PlaceSegment::Index { index: row, ty },
        ] if place.root_ty().is_matrix() => read_matrix_element(ctx, span, value, column, row, ty),
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

pub(in crate::lower) fn temp_copy(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    span: Span,
) -> Result<LowerValue, Diagnostic> {
    let mut lanes = Vec::new();
    for (lane, ty) in value.lanes.iter().zip(scalar_ir_types(&value.ty)?) {
        let dst = ctx.fb.alloc_vreg(ty);
        ctx.fb.push(LpirOp::Copy { dst, src: *lane });
        lanes.push(dst);
    }
    if lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "temporary copy lane mismatch"));
    }
    Ok(LowerValue {
        ty: value.ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn one_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let base = scalar_base_type(ty).unwrap_or_else(|| ty.clone());
    let mut lanes = Vec::new();
    for _ in 0..scalar_lane_count(ty) {
        let lane = match base {
            LpsType::Float => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::FconstF32 { dst, value: 1.0 });
                dst
            }
            LpsType::Int => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
                dst
            }
            LpsType::UInt => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
                dst
            }
            _ => return Err(Diagnostic::error(span, "unsupported increment type")),
        };
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes,
    })
}

pub(in crate::lower) fn copy_value(
    ctx: &mut LowerCtx<'_>,
    dst: LowerValue,
    src: LowerValue,
    span: Span,
) -> Result<(), Diagnostic> {
    if dst.lanes.len() != src.lanes.len() {
        return Err(Diagnostic::error(span, "copy lane count mismatch"));
    }
    for (dst, src) in dst.lanes.iter().zip(src.lanes.iter()) {
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src,
        });
    }
    Ok(())
}
