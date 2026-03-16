//! Conversion from GLSL types to Cranelift IR types.
//!
//! Extracted from semantic::types::Type to keep Cranelift coupling in codegen.

use crate::error::{ErrorCode, GlslError};
use crate::semantic::types::Type;
use cranelift_codegen::ir::Type as IrType;
use cranelift_codegen::ir::types;

/// Convert a GLSL type to the corresponding Cranelift IR type for codegen.
///
/// Returns an error if the type cannot be converted (e.g., Void or unsupported types).
/// For vectors and matrices, returns the element/component type; storage layout
/// is handled separately in the codegen.
pub fn glsl_type_to_cranelift(ty: &Type) -> Result<IrType, GlslError> {
    if ty.is_error() {
        return Err(GlslError::new(
            ErrorCode::E0109,
            "Error type has no Cranelift representation",
        ));
    }
    match ty {
        Type::Bool => Ok(types::I8),
        Type::Int => Ok(types::I32),
        Type::UInt => Ok(types::I32),
        Type::Float => Ok(types::F32),
        Type::Void => Err(GlslError::new(
            ErrorCode::E0109,
            "Void type has no Cranelift representation",
        )),
        Type::Mat2 | Type::Mat3 | Type::Mat4 => Ok(types::F32),
        Type::Vec2 | Type::Vec3 | Type::Vec4 => Ok(types::F32),
        Type::UVec2 | Type::UVec3 | Type::UVec4 => Ok(types::I32),
        Type::Array(element_ty, _) => glsl_type_to_cranelift(element_ty),
        _ => Err(GlslError::new(
            ErrorCode::E0109,
            alloc::format!("Type not yet supported for codegen: {ty:?}"),
        )),
    }
}
