use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};

use super::scalar::{scalar_base_type, scalar_lane_count};

const RULES: LayoutRules = LayoutRules::Std430;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TypeShape {
    pub(super) ty: LpsType,
    pub(super) kind: TypeShapeKind,
    pub(super) lane_count: usize,
    pub(super) byte_size: usize,
    pub(super) byte_align: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TypeShapeKind {
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
    Struct {
        fields: Vec<FieldShape>,
    },
    Texture2D,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldShape {
    pub(super) name: String,
    pub(super) ty: LpsType,
    pub(super) lane_offset: usize,
    pub(super) lane_count: usize,
    pub(super) byte_offset: usize,
}

impl TypeShape {
    pub(super) fn new(ty: &LpsType) -> Self {
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
            LpsType::Struct { members, .. } => {
                let mut byte_offset = 0usize;
                let mut lane_offset = 0usize;
                let mut fields = Vec::with_capacity(members.len());
                for (index, member) in members.iter().enumerate() {
                    let align = type_alignment(&member.ty, RULES);
                    byte_offset = round_up(byte_offset, align);
                    let lane_count = scalar_lane_count(&member.ty);
                    fields.push(FieldShape {
                        name: member
                            .name
                            .clone()
                            .unwrap_or_else(|| alloc::format!("_{index}")),
                        ty: member.ty.clone(),
                        lane_offset,
                        lane_count,
                        byte_offset,
                    });
                    byte_offset += type_size(&member.ty, RULES);
                    lane_offset += lane_count;
                }
                TypeShapeKind::Struct { fields }
            }
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

    pub(super) fn field(&self, name: &str) -> Option<&FieldShape> {
        let TypeShapeKind::Struct { fields } = &self.kind else {
            return None;
        };
        fields.iter().find(|field| field.name == name)
    }

    pub(super) fn array_element(&self) -> Option<(&LpsType, u32, usize)> {
        match &self.kind {
            TypeShapeKind::Array {
                element,
                len,
                stride,
            } => Some((element, *len, *stride)),
            _ => None,
        }
    }

    pub(super) fn matrix_column(&self) -> Option<&LpsType> {
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
