use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::hir::{HirExpr, scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue, lower_expr};
use super::single_lane;

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

pub(super) fn assign_index_target(
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

pub(super) fn lower_index_field(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    base: LowerValue,
    index: &HirExpr,
    element_ty: &LpsType,
    array_lane_offset: usize,
    array_lane_count: usize,
    field_lane_offset: usize,
    field_lane_count: usize,
    field_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let index = lower_expr(ctx, index)?;
    let index = single_lane(span, &index)?;
    let element_width = scalar_lane_count(element_ty);
    if element_width == 0 || field_lane_count == 0 {
        return Err(Diagnostic::error(span, "index field has no lanes"));
    }
    let field_ir_types = scalar_ir_types(field_ty)?;
    let element_count = array_lane_count / element_width;
    let mut lanes = Vec::new();
    for component in 0..field_lane_count {
        let base_component = field_lane_offset + component;
        let Some(mut selected) = base.lanes.get(array_lane_offset + base_component).copied() else {
            return Err(Diagnostic::error(span, "index field lane out of range"));
        };
        let result_ir_ty = field_ir_types
            .get(component)
            .copied()
            .ok_or_else(|| Diagnostic::error(span, "index field result has no type"))?;
        for element_index in 1..element_count {
            let src_index = array_lane_offset + element_index * element_width + base_component;
            let Some(lane) = base.lanes.get(src_index) else {
                return Err(Diagnostic::error(
                    span,
                    "index field base lane out of range",
                ));
            };
            let constant = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: constant,
                value: element_index as i32,
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
        ty: field_ty.clone(),
        lanes,
    })
}

pub(super) fn assign_index_field_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: LowerValue,
    index: &HirExpr,
    element_ty: &LpsType,
    array_lane_offset: usize,
    array_lane_count: usize,
    field_lane_offset: usize,
    field_lane_count: usize,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let index = lower_expr(ctx, index)?;
    let index = single_lane(span, &index)?;
    let element_width = scalar_lane_count(element_ty);
    if element_width == 0 || field_lane_count == 0 || field_lane_count != value.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "index field assignment value lane mismatch",
        ));
    }
    let lane_types = scalar_ir_types(&value.ty)?;
    let element_count = array_lane_count / element_width;
    for element_index in 0..element_count {
        let constant = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: constant,
            value: element_index as i32,
        });
        let cond = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ieq {
            dst: cond,
            lhs: index,
            rhs: constant,
        });
        for component in 0..field_lane_count {
            let dst_index =
                array_lane_offset + element_index * element_width + field_lane_offset + component;
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

pub(super) fn lower_index_index(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    base: LowerValue,
    outer_index: &HirExpr,
    outer_ty: &LpsType,
    inner_index: &HirExpr,
    inner_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let outer_index = lower_expr(ctx, outer_index)?;
    let outer_index = single_lane(span, &outer_index)?;
    let inner_index = lower_expr(ctx, inner_index)?;
    let inner_index = single_lane(span, &inner_index)?;
    let outer_width = scalar_lane_count(outer_ty);
    let inner_width = scalar_lane_count(inner_ty);
    if outer_width == 0 || inner_width == 0 {
        return Err(Diagnostic::error(span, "nested index has no lanes"));
    }
    let outer_count = base.lanes.len() / outer_width;
    let inner_count = outer_width / inner_width;
    let result_ir_types = scalar_ir_types(inner_ty)?;
    let mut lanes = Vec::new();
    for component in 0..inner_width {
        let Some(mut selected) = base.lanes.get(component).copied() else {
            return Err(Diagnostic::error(span, "nested index base has no lanes"));
        };
        let result_ir_ty = result_ir_types
            .get(component)
            .copied()
            .ok_or_else(|| Diagnostic::error(span, "nested index result has no type"))?;
        let mut first = true;
        for outer in 0..outer_count {
            let outer_cond = index_eq(ctx, outer_index, outer);
            for inner in 0..inner_count {
                if first {
                    first = false;
                    continue;
                }
                let inner_cond = index_eq(ctx, inner_index, inner);
                let cond = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::Iand {
                    dst: cond,
                    lhs: outer_cond,
                    rhs: inner_cond,
                });
                let src_index = outer * outer_width + inner * inner_width + component;
                let Some(lane) = base.lanes.get(src_index) else {
                    return Err(Diagnostic::error(
                        span,
                        "nested index base lane out of range",
                    ));
                };
                let dst = ctx.fb.alloc_vreg(result_ir_ty);
                ctx.fb.push(LpirOp::Select {
                    dst,
                    cond,
                    if_true: *lane,
                    if_false: selected,
                });
                selected = dst;
            }
        }
        lanes.push(selected);
    }
    Ok(LowerValue {
        ty: inner_ty.clone(),
        lanes,
    })
}

pub(super) fn assign_index_index_target(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: LowerValue,
    outer_index: &HirExpr,
    outer_ty: &LpsType,
    inner_index: &HirExpr,
    inner_ty: &LpsType,
    value: LowerValue,
) -> Result<(), Diagnostic> {
    let outer_index = lower_expr(ctx, outer_index)?;
    let outer_index = single_lane(span, &outer_index)?;
    let inner_index = lower_expr(ctx, inner_index)?;
    let inner_index = single_lane(span, &inner_index)?;
    let outer_width = scalar_lane_count(outer_ty);
    let inner_width = scalar_lane_count(inner_ty);
    if outer_width == 0 || inner_width == 0 || inner_width != value.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "nested index assignment value lane mismatch",
        ));
    }
    let outer_count = dst.lanes.len() / outer_width;
    let inner_count = outer_width / inner_width;
    let lane_types = scalar_ir_types(&value.ty)?;
    for outer in 0..outer_count {
        let outer_cond = index_eq(ctx, outer_index, outer);
        for inner in 0..inner_count {
            let inner_cond = index_eq(ctx, inner_index, inner);
            let cond = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::Iand {
                dst: cond,
                lhs: outer_cond,
                rhs: inner_cond,
            });
            for component in 0..inner_width {
                let dst_index = outer * outer_width + inner * inner_width + component;
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
    }
    Ok(())
}

fn index_eq(ctx: &mut LowerCtx<'_>, index: lpir::VReg, value: usize) -> lpir::VReg {
    let constant = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: constant,
        value: value as i32,
    });
    let cond = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Ieq {
        dst: cond,
        lhs: index,
        rhs: constant,
    });
    cond
}
