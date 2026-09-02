use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};

use super::scalar::{scalar_base_type, scalar_lane_count};

const RULES: LayoutRules = LayoutRules::Std430;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeShape {
    pub(super) ty: LpsType,
    pub(super) kind: TypeShapeKind,
    pub(super) lane_count: usize,
    pub(super) byte_size: usize,
    pub(super) byte_align: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypeShapeKind {
    Void,
    Scalar,
    Vector {
        base: LpsType,
        lanes: usize,
    },
    Matrix {
        columns: usize,
        rows: usize,
        column_ty: LpsType,
    },
    Array {
        element: LpsType,
        len: u32,
        stride: usize,
    },
    Struct,
    Texture2D,
}

/// One struct member's placement, borrowed from the type it came from.
///
/// The single-field answer that [`struct_field`] gives without building a
/// whole [`TypeShape`].
pub(super) struct FieldPlacement<'a> {
    /// Position in the struct's member list.
    pub(super) index: usize,
    pub(super) ty: &'a LpsType,
    pub(super) lane_offset: usize,
    pub(super) lane_count: usize,
    pub(super) byte_offset: usize,
}

/// Resolves one struct member by name.
///
/// Member access asks this per swizzle, per field path segment and per place
/// segment, so it walks the members directly and stops at the match instead
/// of going through [`TypeShape`], which would clone the whole type first.
/// Offsets use the same std430 stepping, so the answers are identical.
pub(super) fn struct_field<'a>(ty: &'a LpsType, name: &str) -> Option<FieldPlacement<'a>> {
    let LpsType::Struct { members, .. } = ty else {
        return None;
    };
    let mut byte_offset = 0usize;
    let mut lane_offset = 0usize;
    for (index, member) in members.iter().enumerate() {
        let align = type_alignment(&member.ty, RULES);
        byte_offset = round_up(byte_offset, align);
        let lane_count = scalar_lane_count(&member.ty);
        if member_name_matches(member.name.as_deref(), index, name) {
            return Some(FieldPlacement {
                index,
                ty: &member.ty,
                lane_offset,
                lane_count,
                byte_offset,
            });
        }
        byte_offset += type_size(&member.ty, RULES);
        lane_offset += lane_count;
    }
    None
}

/// Whether member `index` answers to `name`.
///
/// Unnamed members are addressed as `_{index}` — the same spelling
/// [`TypeShape::new`] formats — matched here without allocating it.
fn member_name_matches(member_name: Option<&str>, index: usize, name: &str) -> bool {
    match member_name {
        Some(member_name) => member_name == name,
        None => {
            let Some(digits) = name.strip_prefix('_') else {
                return false;
            };
            // `format!("_{index}")` never emits a leading zero (except for
            // `_0` itself), so neither does a match.
            if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
                return false;
            }
            digits.parse::<usize>() == Ok(index)
        }
    }
}

impl TypeShape {
    pub(crate) fn new(ty: &LpsType) -> Self {
        let kind = match ty {
            LpsType::Void => TypeShapeKind::Void,
            LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => TypeShapeKind::Scalar,
            LpsType::Vec2
            | LpsType::Vec3
            | LpsType::Vec4
            | LpsType::IVec2
            | LpsType::IVec3
            | LpsType::IVec4
            | LpsType::UVec2
            | LpsType::UVec3
            | LpsType::UVec4
            | LpsType::BVec2
            | LpsType::BVec3
            | LpsType::BVec4 => TypeShapeKind::Vector {
                base: scalar_base_type(ty).unwrap_or_else(|| ty.clone()),
                lanes: scalar_lane_count(ty),
            },
            LpsType::Mat2 | LpsType::Mat3 | LpsType::Mat4 => {
                let (columns, rows) = ty.matrix_dims().unwrap_or((0, 0));
                TypeShapeKind::Matrix {
                    columns,
                    rows,
                    column_ty: ty.matrix_column_type().unwrap_or(LpsType::Void),
                }
            }
            LpsType::Array { element, len } => TypeShapeKind::Array {
                element: *element.clone(),
                len: *len,
                stride: array_stride(element, RULES),
            },
            // Member placements are resolved on demand by [`struct_field`],
            // so the shape itself carries no per-field table.
            LpsType::Struct { .. } => TypeShapeKind::Struct,
            LpsType::Texture2D => TypeShapeKind::Texture2D,
        };

        Self {
            ty: ty.clone(),
            kind,
            lane_count: scalar_lane_count(ty),
            byte_size: type_size(ty, RULES),
            byte_align: type_alignment(ty, RULES),
        }
    }

    /// Shape-level spelling of [`struct_field`].
    #[allow(
        dead_code,
        reason = "shape-level field lookup; the hot paths call `struct_field` directly and skip building the shape"
    )]
    pub(super) fn field(&self, name: &str) -> Option<FieldPlacement<'_>> {
        struct_field(&self.ty, name)
    }

    pub(crate) fn array_element(&self) -> Option<(&LpsType, u32, usize)> {
        match &self.kind {
            TypeShapeKind::Array {
                element,
                len,
                stride,
            } => Some((element, *len, *stride)),
            _ => None,
        }
    }

    pub(crate) fn matrix_column(&self) -> Option<&LpsType> {
        match &self.kind {
            TypeShapeKind::Matrix { column_ty, .. } => Some(column_ty),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    use lps_shared::StructMember;

    use super::*;

    #[test]
    fn struct_fields_use_shared_std430_offsets_and_lane_offsets() {
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
                StructMember {
                    name: Some(String::from("c")),
                    ty: LpsType::Float,
                },
            ],
        };
        let shape = TypeShape::new(&ty);
        assert_eq!(shape.byte_size, type_size(&ty, RULES));
        assert_eq!(shape.byte_align, type_alignment(&ty, RULES));
        assert_eq!(shape.field("a").unwrap().byte_offset, 0);
        assert_eq!(shape.field("b").unwrap().byte_offset, 8);
        assert_eq!(shape.field("c").unwrap().byte_offset, 16);
        assert_eq!(shape.field("a").unwrap().lane_offset, 0);
        assert_eq!(shape.field("b").unwrap().lane_offset, 1);
        assert_eq!(shape.field("c").unwrap().lane_offset, 3);
    }

    #[test]
    fn array_shape_uses_shared_stride() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Vec3),
            len: 3,
        };
        let shape = TypeShape::new(&ty);
        assert_eq!(shape.array_element(), Some((&LpsType::Vec3, 3, 12)));
        assert_eq!(shape.byte_size, type_size(&ty, RULES));
    }
}
