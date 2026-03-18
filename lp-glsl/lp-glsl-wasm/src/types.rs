//! GLSL type to WASM ValType mapping.

use alloc::vec::Vec;
use lp_glsl_frontend::{FloatMode, semantic::types::Type};
use wasm_encoder::ValType;

/// Map GLSL type to one or more WASM value types (scalar→1, vector→N components).
pub fn glsl_type_to_wasm_components(ty: &Type, float_mode: FloatMode) -> Vec<ValType> {
    match ty {
        Type::Bool | Type::Int | Type::UInt | Type::Float => {
            alloc::vec![glsl_type_to_wasm(ty, float_mode)]
        }
        Type::Void => alloc::vec![],
        Type::Error
        | Type::Mat2
        | Type::Mat3
        | Type::Mat4
        | Type::Sampler2D
        | Type::Struct(_)
        | Type::Array(_, _) => panic!("WASM codegen: unsupported type {:?}", ty),
        Type::Vec2 | Type::IVec2 | Type::UVec2 | Type::BVec2 => {
            let vt = vector_base_to_wasm(&ty.vector_base_type().unwrap(), float_mode);
            alloc::vec![vt; 2]
        }
        Type::Vec3 | Type::IVec3 | Type::UVec3 | Type::BVec3 => {
            let vt = vector_base_to_wasm(&ty.vector_base_type().unwrap(), float_mode);
            alloc::vec![vt; 3]
        }
        Type::Vec4 | Type::IVec4 | Type::UVec4 | Type::BVec4 => {
            let vt = vector_base_to_wasm(&ty.vector_base_type().unwrap(), float_mode);
            alloc::vec![vt; 4]
        }
    }
}

/// Map vector base type to WASM ValType (Float→F32, Int→I32, etc.).
fn vector_base_to_wasm(ty: &Type, float_mode: FloatMode) -> ValType {
    match ty {
        Type::Int | Type::UInt | Type::Bool => ValType::I32,
        Type::Float => match float_mode {
            FloatMode::Q32 => ValType::I32,
            FloatMode::Float => ValType::F32,
        },
        _ => panic!("WASM codegen: invalid vector base {:?}", ty),
    }
}

/// Map GLSL scalar type to single WASM value type.
/// Panics for vectors; use glsl_type_to_wasm_components for vectors.
pub fn glsl_type_to_wasm(ty: &Type, float_mode: FloatMode) -> ValType {
    match ty {
        Type::Int | Type::UInt | Type::Bool => ValType::I32,
        Type::Float => match float_mode {
            FloatMode::Q32 => ValType::I32, // Q16.16
            FloatMode::Float => ValType::F32,
        },
        Type::Void => unreachable!("void has no WASM value type"),
        Type::Error
        | Type::Vec2
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
        | Type::Mat2
        | Type::Mat3
        | Type::Mat4
        | Type::Sampler2D
        | Type::Struct(_)
        | Type::Array(_, _) => panic!("WASM codegen: unsupported type {:?}", ty),
    }
}
