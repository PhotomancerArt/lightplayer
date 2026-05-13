use alloc::vec::Vec;

use lpir::{IrType, LpirOp};
use lps_shared::LpsType;

use crate::hir::{scalar_ir_types, scalar_lane_count};
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue};
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
    let index = clamp_index(ctx, index, source_count);
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

pub(super) fn assign_index_value(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: LowerValue,
    index: LowerValue,
    ty: &LpsType,
    value: LowerValue,
) -> Result<(), Diagnostic> {
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
    let index = clamp_index(ctx, index, count);
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

fn clamp_index(ctx: &mut LowerCtx<'_>, index: lpir::VReg, count: usize) -> lpir::VReg {
    if count <= 1 {
        let zero = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: zero,
            value: 0,
        });
        return zero;
    }

    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: zero,
        value: 0,
    });
    let below_zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IltS {
        dst: below_zero,
        lhs: index,
        rhs: zero,
    });
    let low_clamped = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: low_clamped,
        cond: below_zero,
        if_true: zero,
        if_false: index,
    });

    let last = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IconstI32 {
        dst: last,
        value: count.saturating_sub(1) as i32,
    });
    let above_last = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::IgtS {
        dst: above_last,
        lhs: low_clamped,
        rhs: last,
    });
    let clamped = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Select {
        dst: clamped,
        cond: above_last,
        if_true: last,
        if_false: low_clamped,
    });
    clamped
}
