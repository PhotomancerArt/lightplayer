use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::scalar::{scalar_base_type, scalar_lane_count};

pub(super) fn access_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    if let Some((offset, field_ty)) = struct_field_lanes(ty, fields) {
        let width = scalar_lane_count(&field_ty);
        return Ok(((offset..offset + width).collect(), field_ty));
    }
    swizzle_lanes(span, ty, fields)
}

fn struct_field_lanes(ty: &LpsType, field: &str) -> Option<(usize, LpsType)> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    let mut offset = 0usize;
    for member in members {
        if member.name.as_deref() == Some(field) {
            return Some((offset, member.ty.clone()));
        }
        offset = offset.saturating_add(scalar_lane_count(&member.ty));
    }
    None
}

fn swizzle_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    let count = scalar_lane_count(ty);
    if count < 2 {
        return Err(Diagnostic::error(span, "swizzle requires vector base"));
    }
    let mut lanes = Vec::new();
    for ch in fields.chars() {
        let lane = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => return Err(Diagnostic::error(span, "unsupported swizzle field")),
        };
        if lane >= count {
            return Err(Diagnostic::error(span, "swizzle lane out of range"));
        }
        lanes.push(lane);
    }
    let base = scalar_base_type(ty).ok_or_else(|| Diagnostic::error(span, "swizzle base type"))?;
    let out_ty = if lanes.len() == 1 {
        base
    } else {
        LpsType::vector_type(&base, lanes.len())
            .ok_or_else(|| Diagnostic::error(span, "unsupported swizzle width"))?
    };
    Ok((lanes, out_ty))
}
