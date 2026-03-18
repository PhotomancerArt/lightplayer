//! WASM rvalue type for type-aware expression emission.

use lp_glsl_frontend::semantic::types::Type;

/// Result of emitting an rvalue: type and stack footprint.
///
/// Drives type-aware binary op dispatch, coercion, and return handling.
#[derive(Debug, Clone)]
pub struct WasmRValue {
    /// GLSL type of the value(s) on the stack.
    pub ty: Type,
    /// Number of WASM values pushed onto the stack.
    /// 1 for scalars, 2-4 for vectors, etc.
    pub stack_count: u32,
}

impl WasmRValue {
    pub fn scalar(ty: Type) -> Self {
        let stack_count = if matches!(ty, Type::Void) { 0 } else { 1 };
        Self { ty, stack_count }
    }

    pub fn from_type(ty: Type) -> Self {
        let stack_count = if matches!(ty, Type::Void) {
            0
        } else if ty.is_vector() {
            ty.component_count().unwrap_or(1) as u32
        } else {
            1
        };
        Self { ty, stack_count }
    }

    pub fn void() -> Self {
        Self {
            ty: Type::Void,
            stack_count: 0,
        }
    }
}
