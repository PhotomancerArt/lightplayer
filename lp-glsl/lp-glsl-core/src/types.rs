use alloc::{boxed::Box, vec::Vec};

/// GLSL type system (copied from `lp-glsl-frontend`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
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
    Array(Box<Type>, usize),
    Error,
}

pub type StructId = usize;

impl Type {
    pub fn is_error(&self) -> bool {
        matches!(self, Type::Error)
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            Type::Int | Type::UInt | Type::Float => true,
            Type::Vec2
            | Type::Vec3
            | Type::Vec4
            | Type::IVec2
            | Type::IVec3
            | Type::IVec4
            | Type::UVec2
            | Type::UVec3
            | Type::UVec4 => true,
            Type::Mat2 | Type::Mat3 | Type::Mat4 => true,
            Type::Array(element_ty, _) => element_ty.is_numeric(),
            _ => false,
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, Type::Bool | Type::Int | Type::UInt | Type::Float)
    }

    pub fn is_vector(&self) -> bool {
        matches!(
            self,
            Type::Vec2
                | Type::Vec3
                | Type::Vec4
                | Type::IVec2
                | Type::IVec3
                | Type::IVec4
                | Type::UVec2
                | Type::UVec3
                | Type::UVec4
                | Type::BVec2
                | Type::BVec3
                | Type::BVec4
        )
    }

    pub fn vector_base_type(&self) -> Option<Type> {
        match self {
            Type::Vec2 | Type::Vec3 | Type::Vec4 => Some(Type::Float),
            Type::IVec2 | Type::IVec3 | Type::IVec4 => Some(Type::Int),
            Type::UVec2 | Type::UVec3 | Type::UVec4 => Some(Type::UInt),
            Type::BVec2 | Type::BVec3 | Type::BVec4 => Some(Type::Bool),
            _ => None,
        }
    }

    pub fn component_count(&self) -> Option<usize> {
        match self {
            Type::Vec2 | Type::IVec2 | Type::UVec2 | Type::BVec2 => Some(2),
            Type::Vec3 | Type::IVec3 | Type::UVec3 | Type::BVec3 => Some(3),
            Type::Vec4 | Type::IVec4 | Type::UVec4 | Type::BVec4 => Some(4),
            _ => None,
        }
    }

    pub fn vector_type(base: &Type, count: usize) -> Option<Type> {
        match (base, count) {
            (Type::Float, 2) => Some(Type::Vec2),
            (Type::Float, 3) => Some(Type::Vec3),
            (Type::Float, 4) => Some(Type::Vec4),
            (Type::Int, 2) => Some(Type::IVec2),
            (Type::Int, 3) => Some(Type::IVec3),
            (Type::Int, 4) => Some(Type::IVec4),
            (Type::UInt, 2) => Some(Type::UVec2),
            (Type::UInt, 3) => Some(Type::UVec3),
            (Type::UInt, 4) => Some(Type::UVec4),
            (Type::Bool, 2) => Some(Type::BVec2),
            (Type::Bool, 3) => Some(Type::BVec3),
            (Type::Bool, 4) => Some(Type::BVec4),
            _ => None,
        }
    }

    pub fn is_matrix(&self) -> bool {
        matches!(self, Type::Mat2 | Type::Mat3 | Type::Mat4)
    }

    pub fn matrix_dims(&self) -> Option<(usize, usize)> {
        match self {
            Type::Mat2 => Some((2, 2)),
            Type::Mat3 => Some((3, 3)),
            Type::Mat4 => Some((4, 4)),
            _ => None,
        }
    }

    pub fn matrix_column_type(&self) -> Option<Type> {
        match self {
            Type::Mat2 => Some(Type::Vec2),
            Type::Mat3 => Some(Type::Vec3),
            Type::Mat4 => Some(Type::Vec4),
            _ => None,
        }
    }

    pub fn matrix_element_count(&self) -> Option<usize> {
        match self {
            Type::Mat2 => Some(4),
            Type::Mat3 => Some(9),
            Type::Mat4 => Some(16),
            _ => None,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(_, _))
    }

    pub fn array_element_type(&self) -> Option<Type> {
        match self {
            Type::Array(element_ty, _) => Some(*element_ty.clone()),
            _ => None,
        }
    }

    pub fn array_dimensions(&self) -> Vec<usize> {
        let mut dims = Vec::new();
        let mut current = self;
        while let Type::Array(element_ty, size) = current {
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
