use lpir::LpirOp;
use lps_shared::LpsType;

use crate::hir::PlaceSegment;
use crate::{Diagnostic, Span};

/// The element type an index segment selects from a value of type `ty`.
fn index_element_type(span: Span, ty: &LpsType) -> Result<LpsType, Diagnostic> {
    crate::hir::index_element_type(ty)
        .ok_or_else(|| Diagnostic::error(span, "index base must be vector, matrix, or array"))
}

/// The type member `member` of struct `ty` projects to.
fn field_type(span: Span, ty: &LpsType, member: u16) -> Result<LpsType, Diagnostic> {
    crate::hir::field_type(ty, member)
        .cloned()
        .ok_or_else(|| Diagnostic::error(span, "field projection of a non-struct"))
}

/// The type a swizzle of `lanes` projects from vector `ty` to.
fn swizzle_type(span: Span, ty: &LpsType, lanes: &[u8]) -> Result<LpsType, Diagnostic> {
    crate::hir::swizzle_type(ty, lanes.len())
        .ok_or_else(|| Diagnostic::error(span, "swizzle of a non-vector"))
}

use super::super::{Lanes, LowerCtx, LowerValue, lower_expr};
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
            member,
            lane_offset,
            lane_count,
            ..
        } => {
            let ty = field_type(span, &value.ty, *member)?;
            read_contiguous_lanes(
                span,
                value,
                *lane_offset as usize,
                *lane_count as usize,
                &ty,
            )?
        }
        PlaceSegment::Swizzle { .. } => {
            let lanes = segment.swizzle_lanes().unwrap_or_default();
            let ty = swizzle_type(span, &value.ty, lanes)?;
            read_lane_map(span, value, lanes, &ty)?
        }
        PlaceSegment::Index { index } => {
            let ty = index_element_type(span, &value.ty)?;
            let index = lower_expr(ctx, *index)?;
            lower_index(ctx, span, value, index, &ty)?
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
            member,
            lane_offset,
            lane_count,
            ..
        } => {
            let ty = field_type(span, &value.ty, *member)?;
            assign_contiguous_lanes(
                ctx,
                span,
                value,
                *lane_offset as usize,
                *lane_count as usize,
                &ty,
                rest,
                assignment,
            )
        }
        PlaceSegment::Swizzle { .. } => {
            let lanes = segment.swizzle_lanes().unwrap_or_default();
            let ty = swizzle_type(span, &value.ty, lanes)?;
            assign_lane_map(ctx, span, value, lanes, &ty, rest, assignment)
        }
        PlaceSegment::Index { index } => {
            let ty = index_element_type(span, &value.ty)?;
            let index = lower_expr(ctx, *index)?;
            if rest.is_empty() {
                assign_index_value(ctx, span, value.clone(), index, &ty, assignment)?;
                return Ok(value);
            }
            let selected = lower_index(ctx, span, value.clone(), index.clone(), &ty)?;
            let updated = assign_segments(ctx, span, selected, rest, assignment)?;
            assign_index_value(ctx, span, value.clone(), index, &ty, updated)?;
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
    lanes: &[u8],
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
        lanes: Lanes::from_slice(lanes),
    })
}

fn read_lane_map(
    span: Span,
    value: LowerValue,
    lanes: &[u8],
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let mut out = Lanes::new();
    for lane in lanes {
        let Some(value_lane) = value.lanes.get(usize::from(*lane)) else {
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
    lanes: &[u8],
    updated: &LowerValue,
) -> Result<(), Diagnostic> {
    if updated.lanes.len() != lanes.len() {
        return Err(Diagnostic::error(span, "swizzle assignment width mismatch"));
    }
    for (dst_lane, src_lane) in lanes.iter().zip(updated.lanes.iter()) {
        let Some(dst) = value.lanes.get(usize::from(*dst_lane)) else {
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
