use alloc::{boxed::Box, vec::Vec};

/// Shader type system (shared with exec / filetests).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LpsType {
    Void,
    Bool,
    Int,
    UInt,
    Float,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
    Mat2,
    Mat3,
    Mat4,
    Sampler2D,
    Struct(StructId),
    Array(Box<LpsType>, usize),
    Error,
}

pub type StructId = usize;

impl LpsType {
    pub fn is_error(&self) -> bool {
        matches!(self, LpsType::Error)
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            LpsType::Int | LpsType::UInt | LpsType::Float => true,
            LpsType::Vec2
            | LpsType::Vec3
            | LpsType::Vec4
            | LpsType::IVec2
            | LpsType::IVec3
            | LpsType::IVec4
            | LpsType::UVec2
            | LpsType::UVec3
            | LpsType::UVec4 => true,
            LpsType::Mat2 | LpsType::Mat3 | LpsType::Mat4 => true,
            LpsType::Array(element_ty, _) => element_ty.is_numeric(),
            _ => false,
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(
            self,
            LpsType::Bool | LpsType::Int | LpsType::UInt | LpsType::Float
        )
    }

    pub fn is_vector(&self) -> bool {
        matches!(
            self,
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
                | LpsType::BVec4
        )
    }

    pub fn vector_base_type(&self) -> Option<LpsType> {
        match self {
            LpsType::Vec2 | LpsType::Vec3 | LpsType::Vec4 => Some(LpsType::Float),
            LpsType::IVec2 | LpsType::IVec3 | LpsType::IVec4 => Some(LpsType::Int),
            LpsType::UVec2 | LpsType::UVec3 | LpsType::UVec4 => Some(LpsType::UInt),
            LpsType::BVec2 | LpsType::BVec3 | LpsType::BVec4 => Some(LpsType::Bool),
            _ => None,
        }
    }

    pub fn component_count(&self) -> Option<usize> {
        match self {
            LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => Some(2),
            LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => Some(3),
            LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => Some(4),
            _ => None,
        }
    }

    pub fn vector_type(base: &LpsType, count: usize) -> Option<LpsType> {
        match (base, count) {
            (LpsType::Float, 2) => Some(LpsType::Vec2),
            (LpsType::Float, 3) => Some(LpsType::Vec3),
            (LpsType::Float, 4) => Some(LpsType::Vec4),
            (LpsType::Int, 2) => Some(LpsType::IVec2),
            (LpsType::Int, 3) => Some(LpsType::IVec3),
            (LpsType::Int, 4) => Some(LpsType::IVec4),
            (LpsType::UInt, 2) => Some(LpsType::UVec2),
            (LpsType::UInt, 3) => Some(LpsType::UVec3),
            (LpsType::UInt, 4) => Some(LpsType::UVec4),
            (LpsType::Bool, 2) => Some(LpsType::BVec2),
            (LpsType::Bool, 3) => Some(LpsType::BVec3),
            (LpsType::Bool, 4) => Some(LpsType::BVec4),
            _ => None,
        }
    }

    pub fn is_matrix(&self) -> bool {
        matches!(self, LpsType::Mat2 | LpsType::Mat3 | LpsType::Mat4)
    }

    pub fn matrix_dims(&self) -> Option<(usize, usize)> {
        match self {
            LpsType::Mat2 => Some((2, 2)),
            LpsType::Mat3 => Some((3, 3)),
            LpsType::Mat4 => Some((4, 4)),
            _ => None,
        }
    }

    pub fn matrix_column_type(&self) -> Option<LpsType> {
        match self {
            LpsType::Mat2 => Some(LpsType::Vec2),
            LpsType::Mat3 => Some(LpsType::Vec3),
            LpsType::Mat4 => Some(LpsType::Vec4),
            _ => None,
        }
    }

    pub fn matrix_element_count(&self) -> Option<usize> {
        match self {
            LpsType::Mat2 => Some(4),
            LpsType::Mat3 => Some(9),
            LpsType::Mat4 => Some(16),
            _ => None,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, LpsType::Array(_, _))
    }

    pub fn array_element_type(&self) -> Option<LpsType> {
        match self {
            LpsType::Array(element_ty, _) => Some(*element_ty.clone()),
            _ => None,
        }
    }

    pub fn array_dimensions(&self) -> Vec<usize> {
        let mut dims = Vec::new();
        let mut current = self;
        while let LpsType::Array(element_ty, size) = current {
            dims.push(*size);
            current = element_ty.as_ref();
        }
        dims
    }

    pub fn array_total_element_count(&self) -> Option<usize> {
        if !self.is_array() {
            return None;
        }
        let dims = self.array_dimensions();
        Some(dims.iter().product())
    }
}
