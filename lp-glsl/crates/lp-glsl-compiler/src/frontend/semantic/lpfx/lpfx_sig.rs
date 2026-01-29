//! Helper functions for converting/calling lpfx functions, handling expanding vec types
//! to scalars expected by the implementation.

use super::lpfx_fn::LpfxFn;
use crate::DecimalFormat;
use crate::semantic::types::Type;
use alloc::vec::Vec;
use cranelift_codegen::ir::{AbiParam, Signature, Type as IrType, types};
use cranelift_codegen::isa::CallConv;

/// Expand vector arguments to individual components
///
/// Takes a slice of (value, type) pairs and expands vectors to their components.
/// Returns a flat list of values.
///
/// # Panics
/// Panics if unsupported types are encountered (only Float, Int, UInt, Vec2, Vec3, Vec4 are supported).
pub fn expand_vector_args(
    param_types: &[Type],
    values: &[cranelift_codegen::ir::Value],
) -> Vec<cranelift_codegen::ir::Value> {
    let mut flat_values = Vec::new();
    let mut value_idx = 0;

    for param_ty in param_types {
        match param_ty {
            Type::Vec2 | Type::IVec2 | Type::UVec2 => {
                // Extract 2 components
                if value_idx + 2 > values.len() {
                    panic!("Not enough values for vec2 parameter");
                }
                flat_values.push(values[value_idx]);
                flat_values.push(values[value_idx + 1]);
                value_idx += 2;
            }
            Type::Vec3 | Type::IVec3 | Type::UVec3 => {
                // Extract 3 components
                if value_idx + 3 > values.len() {
                    panic!("Not enough values for vec3 parameter");
                }
                flat_values.push(values[value_idx]);
                flat_values.push(values[value_idx + 1]);
                flat_values.push(values[value_idx + 2]);
                value_idx += 3;
            }
            Type::Vec4 | Type::IVec4 | Type::UVec4 => {
                // Extract 4 components
                if value_idx + 4 > values.len() {
                    panic!("Not enough values for vec4 parameter");
                }
                flat_values.push(values[value_idx]);
                flat_values.push(values[value_idx + 1]);
                flat_values.push(values[value_idx + 2]);
                flat_values.push(values[value_idx + 3]);
                value_idx += 4;
            }
            Type::Float | Type::Int | Type::UInt => {
                // Scalar - single value
                if value_idx >= values.len() {
                    panic!("Not enough values for scalar parameter");
                }
                flat_values.push(values[value_idx]);
                value_idx += 1;
            }
            _ => {
                panic!("Unsupported parameter type for LPFX function: {param_ty:?}");
            }
        }
    }

    flat_values
}

/// Convert GLSL types to Cranelift types based on decimal format
///
/// For Q32 format, all numeric types are converted to i32.
/// Float → i32 (q32 representation)
/// UInt → i32 (Cranelift representation)
/// Int → i32
///
/// # Panics
/// Panics if unsupported types are encountered.
pub fn convert_to_cranelift_types(
    param_types: &[Type],
    format: DecimalFormat,
) -> Vec<cranelift_codegen::ir::Type> {
    let mut cranelift_types = Vec::new();

    for param_ty in param_types {
        match param_ty {
            Type::Vec2 | Type::IVec2 | Type::UVec2 => {
                // Vec2 expands to 2 components
                match format {
                    DecimalFormat::Q32 => {
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                    }
                    DecimalFormat::Float => {
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                    }
                }
            }
            Type::Vec3 | Type::IVec3 | Type::UVec3 => {
                // Vec3 expands to 3 components
                match format {
                    DecimalFormat::Q32 => {
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                    }
                    DecimalFormat::Float => {
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                    }
                }
            }
            Type::Vec4 | Type::IVec4 | Type::UVec4 => {
                // Vec4 expands to 4 components
                match format {
                    DecimalFormat::Q32 => {
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                        cranelift_types.push(types::I32);
                    }
                    DecimalFormat::Float => {
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                        cranelift_types.push(types::F32);
                    }
                }
            }
            Type::Float => match format {
                DecimalFormat::Q32 => cranelift_types.push(types::I32),
                DecimalFormat::Float => cranelift_types.push(types::F32),
            },
            Type::UInt | Type::Int => {
                // UInt and Int both map to I32 in Cranelift
                cranelift_types.push(types::I32);
            }
            _ => {
                panic!("Unsupported parameter type for LPFX function: {param_ty:?}");
            }
        }
    }

    cranelift_types
}

/// Build Cranelift signature dynamically from function signature
///
/// Expands vector parameters and converts types based on decimal format.
/// Returns a Signature ready for use in Cranelift function calls.
///
/// # Arguments
/// * `func` - The LPFX function definition
/// * `_builtin_id` - Builtin ID (unused, kept for compatibility)
/// * `format` - Decimal format (Q32 or Float)
/// * `pointer_type` - Pointer type for result pointer parameter (required for vector returns)
pub fn build_call_signature(
    func: &LpfxFn,
    _builtin_id: crate::backend::builtins::registry::BuiltinId,
    format: DecimalFormat,
    pointer_type: IrType,
) -> Signature {
    let mut sig = Signature::new(CallConv::SystemV);

    // Handle return type FIRST (before params) - if vector, add result pointer as normal parameter
    let return_type = &func.glsl_sig.return_type;
    if return_type.is_vector() {
        // Vector return: pass result pointer as first normal parameter (not StructReturn)
        // This avoids ABI issues - Rust functions already take result_ptr as first parameter
        sig.params.insert(0, AbiParam::new(pointer_type));
        // Functions with result pointer return void
        sig.returns.clear();
    } else {
        // Scalar return: add return value
        match return_type {
            Type::Float => match format {
                DecimalFormat::Q32 => sig.returns.push(AbiParam::new(types::I32)),
                DecimalFormat::Float => sig.returns.push(AbiParam::new(types::F32)),
            },
            Type::UInt | Type::Int => {
                sig.returns.push(AbiParam::new(types::I32));
            }
            Type::Void => {
                // Void return - no return value
            }
            _ => {
                panic!("Unsupported return type for LPFX function: {return_type:?}");
            }
        }
    }

    // Add parameters, handling out/inout as pointers
    use crate::semantic::functions::ParamQualifier;
    for param in &func.glsl_sig.parameters {
        match param.qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Out/inout parameters: pass as single pointer
                sig.params.push(AbiParam::new(pointer_type));
            }
            ParamQualifier::In => {
                // In parameters: expand to components (existing behavior)
                let cranelift_param_types = convert_to_cranelift_types(&[param.ty.clone()], format);
                for ty in cranelift_param_types {
                    sig.params.push(AbiParam::new(ty));
                }
            }
        }
    }

    sig
}
