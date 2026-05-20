use alloc::vec::Vec;

use lpir::LpirOp;
use lps_shared::LpsType;

use crate::hir::PlaceSegment;
use crate::{Diagnostic, Span};

use super::super::{LowerCtx, LowerValue, lower_expr};
use super::access::copy_value;
use super::index::{assign_index_value, lower_index};

pub(super) fn read_segments(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    segments: &[PlaceSegment],
) -> Result<LowerValue, Diagnostic> {
    let Some((segment, rest)) = segments.split_first() else {
        return Ok(value);
    };
    let value = match segment {
        PlaceSegment::Field {
            lane_offset,
            lane_count,
            ty,
            ..
        } => read_contiguous_lanes(span, value, *lane_offset, *lane_count, ty)?,
        PlaceSegment::Swizzle { lanes, ty, .. } => read_lane_map(span, value, lanes, ty)?,
        PlaceSegment::Index { index, ty } => {
            let index = lower_expr(ctx, *index)?;
            lower_index(ctx, span, value, index, ty)?
        }
    };
    read_segments(ctx, span, value, rest)
}

pub(super) fn assign_segments(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    segments: &[PlaceSegment],
    assignment: LowerValue,
) -> Result<LowerValue, Diagnostic> {
    let Some((segment, rest)) = segments.split_first() else {
        copy_value(ctx, value.clone(), assignment, span)?;
        return Ok(value);
    };
    match segment {
        PlaceSegment::Field {
            lane_offset,
            lane_count,
            ty,
            ..
        } => assign_contiguous_lanes(
            ctx,
            span,
            value,
            *lane_offset,
            *lane_count,
            ty,
            rest,
            assignment,
        ),
        PlaceSegment::Swizzle { lanes, ty, .. } => {
            assign_lane_map(ctx, span, value, lanes, ty, rest, assignment)
        }
        PlaceSegment::Index { index, ty } => {
            let index = lower_expr(ctx, *index)?;
            if rest.is_empty() {
                assign_index_value(ctx, span, value.clone(), index, ty, assignment)?;
                return Ok(value);
            }
            let selected = lower_index(ctx, span, value.clone(), index.clone(), ty)?;
            let updated = assign_segments(ctx, span, selected, rest, assignment)?;
            assign_index_value(ctx, span, value.clone(), index, ty, updated)?;
            Ok(value)
        }
    }
}

fn assign_contiguous_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    lane_offset: usize,
    lane_count: usize,
    ty: &LpsType,
    rest: &[PlaceSegment],
    assignment: LowerValue,
) -> Result<LowerValue, Diagnostic> {
    if rest.is_empty() {
        if assignment.lanes.len() != lane_count {
            return Err(Diagnostic::error(span, "lane assignment width mismatch"));
        }
        copy_back_lanes(
            ctx,
            span,
            &value,
            lane_offset..lane_offset + lane_count,
            &assignment,
        )?;
        return Ok(value);
    }
    let projected = read_contiguous_lanes(span, value.clone(), lane_offset, lane_count, ty)?;
    let updated = assign_segments(ctx, span, projected, rest, assignment)?;
    copy_back_lanes(
        ctx,
        span,
        &value,
        lane_offset..lane_offset + lane_count,
        &updated,
    )?;
    Ok(value)
}

fn assign_lane_map(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    lanes: &[usize],
    ty: &LpsType,
    rest: &[PlaceSegment],
    assignment: LowerValue,
) -> Result<LowerValue, Diagnostic> {
    if rest.is_empty() {
        copy_mapped_lanes(ctx, span, &value, lanes, &assignment)?;
        return Ok(value);
    }
    let projected = read_lane_map(span, value.clone(), lanes, ty)?;
    let updated = assign_segments(ctx, span, projected, rest, assignment)?;
    copy_mapped_lanes(ctx, span, &value, lanes, &updated)?;
    Ok(value)
}

fn read_contiguous_lanes(
    span: Span,
    value: LowerValue,
    lane_offset: usize,
    lane_count: usize,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let end = lane_offset + lane_count;
    let Some(lanes) = value.lanes.get(lane_offset..end) else {
        return Err(Diagnostic::error(span, "lane read out of range"));
    };
    Ok(LowerValue {
        ty: ty.clone(),
        lanes: lanes.to_vec(),
    })
}

fn read_lane_map(
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

fn copy_mapped_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: &LowerValue,
    lanes: &[usize],
    updated: &LowerValue,
) -> Result<(), Diagnostic> {
    if updated.lanes.len() != lanes.len() {
        return Err(Diagnostic::error(span, "swizzle assignment width mismatch"));
    }
    for (dst_lane, src_lane) in lanes.iter().zip(updated.lanes.iter()) {
        let Some(dst) = value.lanes.get(*dst_lane) else {
            return Err(Diagnostic::error(
                span,
                "swizzle assignment lane out of range",
            ));
        };
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src_lane,
        });
    }
    Ok(())
}

fn copy_back_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    dst: &LowerValue,
    dst_lanes: core::ops::Range<usize>,
    value: &LowerValue,
) -> Result<(), Diagnostic> {
    if dst_lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "lane assignment width mismatch"));
    }
    for (dst_lane, src_lane) in dst_lanes.zip(value.lanes.iter()) {
        let Some(dst) = dst.lanes.get(dst_lane) else {
            return Err(Diagnostic::error(span, "lane assignment out of range"));
        };
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src_lane,
        });
    }
    Ok(())
}
