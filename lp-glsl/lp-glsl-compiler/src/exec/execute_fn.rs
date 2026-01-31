//! Test utilities for executing GLSL functions.
//!
//! This module provides shared execution logic used by both filetests and runtime tests.

use crate::exec::GlslExecutable;
use crate::exec::glsl_value::GlslValue;
use crate::frontend::semantic::types::Type;
#[cfg(feature = "std")]
use anyhow::Result;

/// Execute a function by name with arguments and return the result as a GlslValue.
///
/// This function handles calling the appropriate method based on the return type
/// and automatically formats errors with emulator state and CLIF IR when available.
#[cfg(feature = "std")]
pub fn execute_function(
    executable: &mut dyn GlslExecutable,
    name: &str,
    args: &[GlslValue],
) -> Result<GlslValue> {
    // Try to get the signature to determine return type
    let sig = executable
        .get_function_signature(name)
        .ok_or_else(|| anyhow::anyhow!("function '{name}' not found"))?;

    // Helper to add emulator state to error if available
    fn format_error(e: crate::error::GlslError, executable: &dyn GlslExecutable) -> anyhow::Error {
        // Use {:#} format to preserve location and span_text formatting
        // Notes are already included in the Display implementation
        let error_msg = format!("{e:#}");
        if let Some(state) = executable.format_emulator_state() {
            anyhow::anyhow!("{error_msg}{state}")
        } else {
            anyhow::anyhow!("{error_msg}")
        }
    }

    // Call function based on return type
    match &sig.return_type {
        Type::Float => executable
            .call_f32(name, args)
            .map(GlslValue::F32)
            .map_err(|e| format_error(e, executable)),
        Type::Int => executable
            .call_i32(name, args)
            .map(GlslValue::I32)
            .map_err(|e| format_error(e, executable)),
        Type::UInt => executable
            .call_i32(name, args)
            .map(|i| GlslValue::U32(i as u32)) // Convert i32 to u32 (bit pattern preserved)
            .map_err(|e| format_error(e, executable)),
        Type::Bool => executable
            .call_bool(name, args)
            .map(GlslValue::Bool)
            .map_err(|e| format_error(e, executable)),
        Type::Vec2 => executable
            .call_vec(name, args, 2)
            .map(|v| GlslValue::Vec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::Vec3 => executable
            .call_vec(name, args, 3)
            .map(|v| GlslValue::Vec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::Vec4 => executable
            .call_vec(name, args, 4)
            .map(|v| GlslValue::Vec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::Mat2 => executable
            .call_mat(name, args, 2, 2)
            .map(|v| {
                // Convert flat array from emulator (column-major storage) to Mat2x2
                // Input: v = [col0_row0, col0_row1, col1_row0, col1_row1] = [v[0], v[1], v[2], v[3]]
                // Output: m[col][row] format (column-major)
                // Column 0: [v[0], v[1]] = [col0_row0, col0_row1]
                // Column 1: [v[2], v[3]] = [col1_row0, col1_row1]
                // This creates: [[v[0], v[1]], [v[2], v[3]]] = [[col0_row0, col0_row1], [col1_row0, col1_row1]]
                GlslValue::Mat2x2([[v[0], v[1]], [v[2], v[3]]])
            })
            .map_err(|e| format_error(e, executable)),
        Type::Mat3 => executable
            .call_mat(name, args, 3, 3)
            .map(|v| {
                // Convert flat array from emulator (column-major storage) to Mat3x3
                // Input: v = [col0_row0, col0_row1, col0_row2, col1_row0, col1_row1, col1_row2, col2_row0, col2_row1, col2_row2]
                // Output: m[col][row] format (column-major)
                // Column 0: [v[0], v[1], v[2]] = [col0_row0, col0_row1, col0_row2]
                // Column 1: [v[3], v[4], v[5]] = [col1_row0, col1_row1, col1_row2]
                // Column 2: [v[6], v[7], v[8]] = [col2_row0, col2_row1, col2_row2]
                // Pattern: m[col][row] = v[col * rows + row]
                GlslValue::Mat3x3([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]])
            })
            .map_err(|e| format_error(e, executable)),
        Type::Mat4 => executable
            .call_mat(name, args, 4, 4)
            .map(|v| {
                // Convert flat array from emulator (column-major storage) to Mat4x4
                // Input: v = 16 elements in column-major order
                // [col0_row0, col0_row1, col0_row2, col0_row3, col1_row0, col1_row1, col1_row2, col1_row3, ...]
                // Output: m[col][row] format (column-major)
                // Column 0: [v[0], v[1], v[2], v[3]]
                // Column 1: [v[4], v[5], v[6], v[7]]
                // Column 2: [v[8], v[9], v[10], v[11]]
                // Column 3: [v[12], v[13], v[14], v[15]]
                // Pattern: m[col][row] = v[col * rows + row]
                GlslValue::Mat4x4([
                    [v[0], v[1], v[2], v[3]],
                    [v[4], v[5], v[6], v[7]],
                    [v[8], v[9], v[10], v[11]],
                    [v[12], v[13], v[14], v[15]],
                ])
            })
            .map_err(|e| format_error(e, executable)),
        // Integer vectors: stored as plain i32, read directly without fixed-point scaling
        Type::IVec2 => executable
            .call_ivec(name, args, 2)
            .map(|v| GlslValue::IVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::IVec3 => executable
            .call_ivec(name, args, 3)
            .map(|v| GlslValue::IVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::IVec4 => executable
            .call_ivec(name, args, 4)
            .map(|v| GlslValue::IVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        // Boolean vectors: stored as i8 in StructReturn, read as i8 and convert to bool
        Type::BVec2 => executable
            .call_bvec(name, args, 2)
            .map(|v| GlslValue::BVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::BVec3 => executable
            .call_bvec(name, args, 3)
            .map(|v| GlslValue::BVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::BVec4 => executable
            .call_bvec(name, args, 4)
            .map(|v| GlslValue::BVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        // Unsigned integer vectors: stored as plain i32 (interpreted as u32), read directly without fixed-point scaling
        Type::UVec2 => executable
            .call_uvec(name, args, 2)
            .map(|v| GlslValue::UVec2([v[0], v[1]]))
            .map_err(|e| format_error(e, executable)),
        Type::UVec3 => executable
            .call_uvec(name, args, 3)
            .map(|v| GlslValue::UVec3([v[0], v[1], v[2]]))
            .map_err(|e| format_error(e, executable)),
        Type::UVec4 => executable
            .call_uvec(name, args, 4)
            .map(|v| GlslValue::UVec4([v[0], v[1], v[2], v[3]]))
            .map_err(|e| format_error(e, executable)),
        Type::Void => executable
            .call_void(name, args)
            .map(|_| GlslValue::F32(0.0))
            .map_err(|e| format_error(e, executable)),
        _ => anyhow::bail!("unsupported return type: {:?}", sig.return_type),
    }
}

/// Execute main() and return the result as a GlslValue.
///
/// This is a convenience wrapper around execute_function for backward compatibility.
#[cfg(feature = "std")]
pub fn execute_main(executable: &mut dyn GlslExecutable) -> Result<GlslValue> {
    execute_function(executable, "main", &[])
}
