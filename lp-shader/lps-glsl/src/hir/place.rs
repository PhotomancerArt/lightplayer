use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsType;

use crate::{Diagnostic, Span};

use super::arena::ExprId;
use super::scalar::{scalar_base_type, scalar_lane_count};
use super::shape::struct_field;
use super::type_table::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "call-actual places land when aggregate out/inout lowering moves here"
)]
pub(crate) enum AccessMode {
    Read,
    Write,
    CallActual,
}

/// One typed place under construction: a root plus the projections applied
/// to it. Built by value during typing ([`TypeCtx::build_place`]) and then
/// pushed into the arena, where it becomes a [`PlaceRecord`] whose segments
/// live in the arena's flat segment list — no heap vector per place.
///
/// `ty` is the type of the whole path (the last projection's result). The
/// root and the segments deliberately carry no type of their own: the
/// root's is in the function's local/param tables or the module's
/// uniform/global tables, a field's follows from the struct it projects
/// ([`field_type`]), a swizzle's from the vector it projects
/// ([`swizzle_type`]) and an index's from the type it indexes
/// ([`index_element_type`]). Storing them here made every `a[i].f`
/// reference hold copies of `a`'s full type — for a struct-array global
/// that is the whole struct, member names and all — and the arena keeps
/// every place until the function lowers. That was the meteor sim compile
/// exhausting the ESP32-C6 heap (2026-09-01), and the 104 B segment with
/// its own `String` and `LpsType` was the next 19 KB of it (2026-09-02).
#[derive(Debug, Clone)]
pub(crate) struct HirPlace {
    pub(crate) root: PlaceRoot,
    pub(crate) segments: Vec<PlaceSegment>,
    pub(crate) ty: LpsType,
}

/// A place as the arena stores it: the segments are a span into the
/// arena's segment list ([`HirArena::place_segments`]) and the type an id
/// into its type table.
#[derive(Debug, Clone)]
pub(crate) struct PlaceRecord {
    pub(crate) root: PlaceRoot,
    pub(crate) segments: SegmentList,
    pub(crate) ty: TypeId,
}

/// A place as readers see it: the type resolved through the table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaceRef<'a> {
    pub(crate) root: &'a PlaceRoot,
    pub(crate) ty: &'a LpsType,
}

/// A span of [`PlaceSegment`]s in the arena's segment list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SegmentList {
    pub(super) start: u32,
    pub(super) len: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlaceRoot {
    Local { local: usize },
    Param { param: usize },
    Uniform { name: String, byte_offset: u32 },
    Global { name: String, byte_offset: u32 },
}

/// One projection step. `Copy` and 16 bytes: the member is an index into
/// the struct being projected, a swizzle is at most four lane numbers, and
/// the projected type is derived from the type it applies to rather than
/// stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceSegment {
    Field {
        member: u16,
        lane_offset: u32,
        lane_count: u32,
        byte_offset: u32,
    },
    Swizzle {
        lanes: [u8; 4],
        len: u8,
    },
    Index {
        index: ExprId,
    },
}

impl PlaceSegment {
    /// The lanes a swizzle segment selects (`None` for other segments).
    pub(crate) fn swizzle_lanes(&self) -> Option<&[u8]> {
        match self {
            PlaceSegment::Swizzle { lanes, len } => Some(&lanes[..usize::from(*len)]),
            _ => None,
        }
    }
}

impl HirPlace {
    pub(super) fn local(local: usize, ty: LpsType) -> Self {
        Self {
            root: PlaceRoot::Local { local },
            segments: Vec::new(),
            ty,
        }
    }

    pub(super) fn param(param: usize, ty: LpsType) -> Self {
        Self {
            root: PlaceRoot::Param { param },
            segments: Vec::new(),
            ty,
        }
    }

    pub(super) fn uniform(name: String, byte_offset: u32, ty: LpsType) -> Self {
        Self {
            root: PlaceRoot::Uniform { name, byte_offset },
            segments: Vec::new(),
            ty,
        }
    }

    pub(super) fn global(name: String, byte_offset: u32, ty: LpsType) -> Self {
        Self {
            root: PlaceRoot::Global { name, byte_offset },
            segments: Vec::new(),
            ty,
        }
    }

    pub(super) fn push_field(&mut self, span: Span, name: &str) -> Result<(), Diagnostic> {
        if let Some(field) = struct_field(&self.ty, name) {
            let member = u16::try_from(field.index)
                .map_err(|_| Diagnostic::error(span, "struct has too many members"))?;
            let segment = PlaceSegment::Field {
                member,
                lane_offset: field.lane_offset as u32,
                lane_count: field.lane_count as u32,
                byte_offset: field.byte_offset as u32,
            };
            self.ty = field.ty.clone();
            self.segments.push(segment);
            return Ok(());
        }
        let (relative_lanes, ty) = swizzle_lanes(span, &self.ty, name)?;
        let mut lanes = [0u8; 4];
        if relative_lanes.len() > lanes.len() {
            return Err(Diagnostic::error(span, "swizzle selects too many lanes"));
        }
        for (slot, lane) in lanes.iter_mut().zip(relative_lanes.iter()) {
            *slot = *lane as u8;
        }
        self.ty = ty;
        self.segments.push(PlaceSegment::Swizzle {
            lanes,
            len: relative_lanes.len() as u8,
        });
        Ok(())
    }

    pub(super) fn push_index(&mut self, index: ExprId, span: Span) -> Result<(), Diagnostic> {
        let Some(ty) = index_element_type(&self.ty) else {
            return Err(Diagnostic::error(
                span,
                "index base must be vector, matrix, or array",
            ));
        };
        self.ty = ty;
        self.segments.push(PlaceSegment::Index { index });
        Ok(())
    }
}

/// The type member `member` of struct `ty` has, borrowed from the struct.
/// `None` when `ty` is not a struct or the index is out of range.
pub(crate) fn field_type(ty: &LpsType, member: u16) -> Option<&LpsType> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    members.get(usize::from(member)).map(|m| &m.ty)
}

/// The name member `member` of struct `ty` answers to: its declared name,
/// or `_{index}` for an unnamed member (the spelling [`struct_field`]
/// matches).
pub(crate) fn field_name(ty: &LpsType, member: u16) -> Option<String> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    let m = members.get(usize::from(member))?;
    Some(match &m.name {
        Some(name) => name.clone(),
        None => alloc::format!("_{member}"),
    })
}

/// The type a swizzle of `len` lanes of vector `ty` has: the scalar base for
/// one lane, the vector of that base otherwise.
pub(crate) fn swizzle_type(ty: &LpsType, len: usize) -> Option<LpsType> {
    let base = scalar_base_type(ty)?;
    if len == 1 {
        Some(base)
    } else {
        LpsType::vector_type(&base, len)
    }
}

/// The type `ty[i]` has: a matrix's column, an array's element, or a
/// vector's scalar. `None` when `ty` cannot be indexed.
///
/// Shared by the HIR build (the place's own type narrowing) and lowering
/// (which re-derives the element type from the value it indexes instead of
/// reading one stored on the segment). The matrix/array/vector questions are
/// answered by the type itself; building a shape table here would clone the
/// whole type (and, for structs, every field) per index to ask three
/// predicates.
pub(crate) fn index_element_type(ty: &LpsType) -> Option<LpsType> {
    if let Some(column_ty) = ty.matrix_column_type() {
        return Some(column_ty);
    }
    if let LpsType::Array { element, .. } = ty {
        return Some((**element).clone());
    }
    scalar_base_type(ty)
}

impl PlaceRoot {
    pub(crate) fn is_writable(&self) -> bool {
        !matches!(self, PlaceRoot::Uniform { .. })
    }
}

pub(super) fn access_lanes(
    span: Span,
    ty: &LpsType,
    fields: &str,
) -> Result<(Vec<usize>, LpsType), Diagnostic> {
    if let Some(field) = struct_field(ty, fields) {
        return Ok((
            (field.lane_offset..field.lane_offset + field.lane_count).collect(),
            field.ty.clone(),
        ));
    }
    swizzle_lanes(span, ty, fields)
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

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    use lps_shared::StructMember;

    use super::*;
    use crate::hir::{HirExprKind, TypeTable, TypedArena};

    #[test]
    fn place_struct_field_keeps_lane_and_byte_metadata() {
        let ty = LpsType::Struct {
            name: Some(String::from("S")),
            members: vec![
                StructMember {
                    name: Some(String::from("a")),
                    ty: LpsType::Float,
                },
                StructMember {
                    name: Some(String::from("b")),
                    ty: LpsType::Vec2,
                },
            ],
        };
        let mut place = local_place(0, ty);
        place.push_field(Span::new(0, 1), "b").unwrap();
        assert_eq!(place.ty, LpsType::Vec2);
        let [
            PlaceSegment::Field {
                member,
                byte_offset,
                lane_offset,
                lane_count,
            },
        ] = place.segments.as_slice()
        else {
            panic!("expected one field segment");
        };
        assert_eq!(
            (*member, *byte_offset, *lane_offset, *lane_count),
            (1, 8, 1, 2)
        );
    }

    #[test]
    fn place_swizzle_projects_root_lanes() {
        let mut place = local_place(0, LpsType::Vec4);
        place.push_field(Span::new(0, 2), "zy").unwrap();
        assert_eq!(place.ty, LpsType::Vec2);
        let [segment] = place.segments.as_slice() else {
            panic!("expected one swizzle segment");
        };
        assert_eq!(segment.swizzle_lanes(), Some(&[2u8, 1][..]));
    }

    #[test]
    fn place_array_index_switches_to_dynamic_path() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Vec3),
            len: 2,
        };
        let mut types = TypeTable::default();
        let mut arena = TypedArena::new(&mut types);
        let mut place = local_place(0, ty);
        let index = int_expr(&mut arena, 1);
        place.push_index(index, Span::new(0, 1)).unwrap();
        assert_eq!(place.ty, LpsType::Vec3);
        assert_eq!(place.segments.len(), 1);
    }

    fn local_place(local: usize, ty: LpsType) -> HirPlace {
        HirPlace::local(local, ty)
    }

    fn int_expr(arena: &mut TypedArena<'_>, value: i32) -> ExprId {
        arena.push_expr(
            Span::new(0, 1),
            LpsType::Int,
            HirExprKind::IntLiteral(value),
        )
    }
}
