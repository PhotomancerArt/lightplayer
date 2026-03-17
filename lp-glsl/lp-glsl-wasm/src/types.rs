//! GLSL type to WASM ValType mapping.

use lp_glsl_frontend::{FloatMode, semantic::types::Type};
use wasm_encoder::ValType;

/// Map GLSL type to WASM value type.
///
/// Phase ii supports scalars only: int, uint, float (Q32→i32), bool.
/// Vectors and matrices are out of scope.
pub fn glsl_type_to_wasm(ty: &Type, float_mode: FloatMode) -> ValType {
    match ty {
        Type::Int | Type::UInt | Type::Bool => ValType::I32,
        Type::Float => match float_mode {
            FloatMode::Q32 => ValType::I32, // Q16.16
            FloatMode::Float => ValType::F32,
        },
        Type::Void => {
            unreachable!("void has no WASM value type")
        }
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
        | Type::Array(_, _) => {
            // Phase ii: unsupported
            panic!("WASM codegen: unsupported type {:?}", ty)
        }
    }
}
