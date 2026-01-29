//! LPFX Function Registry
//!
//! Provides lookup functions for LPFX functions from the registry.

use super::lpfx_fn::LpfxFn;
use crate::semantic::types::Type;
use alloc::{format, string::String, vec::Vec};

/// Check if a function name is an LPFX function
///
/// Returns `true` if the name starts with "lpfx_".
pub fn is_lpfx_fn(name: &str) -> bool {
    name.starts_with("lpfx_")
}

/// Find an LPFX function by its GLSL name
///
/// Returns `None` if the function is not found in the registry.
/// Get cached functions array
fn get_cached_functions() -> &'static [LpfxFn] {
    super::lpfx_fns::lpfx_fns()
}

/// Find an LPFX function by its GLSL name and argument types (overload resolution)
///
/// Performs overload resolution by finding all functions with matching name,
/// then selecting the one with exact parameter type match.
///
/// # Arguments
/// * `name` - GLSL function name (e.g., "lpfx_hsv2rgb")
/// * `arg_types` - Argument types for overload resolution
///
/// # Returns
/// * `Some(function)` if exactly one matching overload is found
/// * `None` if no match or ambiguous (multiple exact matches)
pub fn find_lpfx_fn(name: &str, arg_types: &[Type]) -> Option<&'static LpfxFn> {
    // Find all functions with matching name
    let candidates: Vec<&LpfxFn> = get_cached_functions()
        .iter()
        .filter(|f| f.glsl_sig.name == name)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Filter to functions with matching parameter count
    let matching_count: Vec<&LpfxFn> = candidates
        .into_iter()
        .filter(|f| f.glsl_sig.parameters.len() == arg_types.len())
        .collect();

    if matching_count.is_empty() {
        return None;
    }

    // Find exact type matches
    let exact_matches: Vec<&LpfxFn> = matching_count
        .into_iter()
        .filter(|f| matches_signature(f, arg_types))
        .collect();

    // Return first match, or None if ambiguous (multiple matches) or no match
    if exact_matches.len() == 1 {
        Some(exact_matches[0])
    } else {
        None
    }
}

/// Check if a function signature matches the given argument types
///
/// Performs exact type matching:
/// - Scalar types: exact match required
/// - Vector types: exact match required (including component count)
fn matches_signature(func: &LpfxFn, arg_types: &[Type]) -> bool {
    if func.glsl_sig.parameters.len() != arg_types.len() {
        return false;
    }

    for (param, arg_ty) in func.glsl_sig.parameters.iter().zip(arg_types) {
        // Exact type match required
        if param.ty != *arg_ty {
            return false;
        }
    }

    true
}

/// Find an LPFX function by BuiltinId
///
/// Returns `None` if the function is not found in the registry.
pub fn find_lpfx_fn_by_builtin_id(
    builtin_id: crate::backend::builtins::registry::BuiltinId,
) -> Option<&'static LpfxFn> {
    for func in get_cached_functions().iter() {
        match &func.impls {
            super::lpfx_fn::LpfxFnImpl::NonDecimal(id) if *id == builtin_id => {
                return Some(func);
            }
            super::lpfx_fn::LpfxFnImpl::Decimal {
                float_impl,
                q32_impl,
            } => {
                if *float_impl == builtin_id || *q32_impl == builtin_id {
                    return Some(func);
                }
            }
            _ => {}
        }
    }
    None
}

/// Check if an LPFX function call is valid and return the return type
///
/// Uses overload resolution to find the matching function, then returns its return type.
///
/// # Returns
/// - `Ok(return_type)` if the call is valid
/// - `Err(error_message)` if the call is invalid
pub fn check_lpfx_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String> {
    let func = find_lpfx_fn(name, arg_types).ok_or_else(|| {
        format!("no matching overload for LPFX function `{name}` with argument types {arg_types:?}")
    })?;

    Ok(func.glsl_sig.return_type.clone())
}

/// Get the BuiltinId for a function with a specific decimal format
///
/// Returns `None` if no implementation exists for the given format.
pub fn get_builtin_id_for_format(
    func: &'static LpfxFn,
    format: crate::DecimalFormat,
) -> Option<crate::backend::builtins::registry::BuiltinId> {
    match &func.impls {
        super::lpfx_fn::LpfxFnImpl::NonDecimal(builtin_id) => Some(*builtin_id),
        super::lpfx_fn::LpfxFnImpl::Decimal {
            float_impl,
            q32_impl,
        } => match format {
            crate::DecimalFormat::Float => Some(*float_impl),
            crate::DecimalFormat::Q32 => Some(*q32_impl),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_non_overloaded_function() {
        // Test finding a non-overloaded function
        let func = find_lpfx_fn("lpfx_hash", &[Type::UInt, Type::UInt]);
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.glsl_sig.name, "lpfx_hash");
        assert_eq!(func.glsl_sig.parameters.len(), 2);
    }

    #[test]
    fn test_find_overloaded_function_vec3() {
        // Test finding vec3 overload of hsv2rgb
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3]);
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.glsl_sig.name, "lpfx_hsv2rgb");
        assert_eq!(func.glsl_sig.parameters.len(), 1);
        assert_eq!(func.glsl_sig.parameters[0].ty, Type::Vec3);
        assert_eq!(func.glsl_sig.return_type, Type::Vec3);
    }

    #[test]
    fn test_find_overloaded_function_vec4() {
        // Test finding vec4 overload of hsv2rgb
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec4]);
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.glsl_sig.name, "lpfx_hsv2rgb");
        assert_eq!(func.glsl_sig.parameters.len(), 1);
        assert_eq!(func.glsl_sig.parameters[0].ty, Type::Vec4);
        assert_eq!(func.glsl_sig.return_type, Type::Vec4);
    }

    #[test]
    fn test_find_function_wrong_parameter_count() {
        // Test with wrong parameter count
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3, Type::Float]);
        assert!(func.is_none());
    }

    #[test]
    fn test_find_function_wrong_type() {
        // Test with wrong type (vec2 instead of vec3)
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec2]);
        assert!(func.is_none());
    }

    #[test]
    fn test_find_function_scalar_vs_vector() {
        // Test that scalar and vector types don't match
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Float]);
        assert!(func.is_none());
    }

    #[test]
    fn test_find_unknown_function() {
        // Test with unknown function name
        let func = find_lpfx_fn("lpfx_unknown", &[Type::Float]);
        assert!(func.is_none());
    }

    #[test]
    fn test_check_lpfx_fn_call_vec3() {
        // Test check_lpfx_fn_call with vec3 overload
        let result = check_lpfx_fn_call("lpfx_hsv2rgb", &[Type::Vec3]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Vec3);
    }

    #[test]
    fn test_check_lpfx_fn_call_vec4() {
        // Test check_lpfx_fn_call with vec4 overload
        let result = check_lpfx_fn_call("lpfx_hsv2rgb", &[Type::Vec4]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Vec4);
    }

    #[test]
    fn test_check_lpfx_fn_call_no_match() {
        // Test check_lpfx_fn_call with no matching overload
        let result = check_lpfx_fn_call("lpfx_hsv2rgb", &[Type::Vec2]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no matching overload"));
    }

    #[test]
    fn test_matches_signature_exact_match() {
        // Test matches_signature with exact match
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3]).unwrap();
        assert!(matches_signature(func, &[Type::Vec3]));
    }

    #[test]
    fn test_matches_signature_type_mismatch() {
        // Test matches_signature with type mismatch
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3]).unwrap();
        assert!(!matches_signature(func, &[Type::Vec4]));
        assert!(!matches_signature(func, &[Type::Vec2]));
        assert!(!matches_signature(func, &[Type::Float]));
    }

    #[test]
    fn test_matches_signature_count_mismatch() {
        // Test matches_signature with parameter count mismatch
        let func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3]).unwrap();
        assert!(!matches_signature(func, &[]));
        assert!(!matches_signature(func, &[Type::Vec3, Type::Float]));
    }

    #[test]
    fn test_overload_resolution_distinguishes_types() {
        // Test that overload resolution correctly distinguishes vec3 from vec4
        let vec3_func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec3]).unwrap();
        let vec4_func = find_lpfx_fn("lpfx_hsv2rgb", &[Type::Vec4]).unwrap();

        // They should be different functions (different return types)
        assert_ne!(
            vec3_func.glsl_sig.return_type,
            vec4_func.glsl_sig.return_type
        );
        assert_eq!(vec3_func.glsl_sig.return_type, Type::Vec3);
        assert_eq!(vec4_func.glsl_sig.return_type, Type::Vec4);
    }

    #[test]
    fn test_is_lpfx_fn() {
        // Test is_lpfx_fn helper
        assert!(is_lpfx_fn("lpfx_hsv2rgb"));
        assert!(is_lpfx_fn("lpfx_hash"));
        assert!(!is_lpfx_fn("hsv2rgb"));
        assert!(!is_lpfx_fn("lpfx"));
        assert!(!is_lpfx_fn(""));
    }

    #[test]
    fn test_find_lpfx_fn_by_builtin_id_f32_to_q32() {
        use crate::backend::builtins::registry::BuiltinId;

        // Test that f32 builtin IDs map to LPFX functions, and we can extract q32_impl
        let f32_builtin = BuiltinId::LpfxSaturateVec3F32;
        let lpfx_fn = find_lpfx_fn_by_builtin_id(f32_builtin);
        assert!(lpfx_fn.is_some());
        let lpfx_fn = lpfx_fn.unwrap();

        // Verify it's the correct function
        assert_eq!(lpfx_fn.glsl_sig.name, "lpfx_saturate");
        assert_eq!(
            lpfx_fn.glsl_sig.return_type,
            crate::semantic::types::Type::Vec3
        );

        // Extract q32_impl
        match &lpfx_fn.impls {
            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::Decimal {
                float_impl,
                q32_impl,
            } => {
                assert_eq!(*float_impl, BuiltinId::LpfxSaturateVec3F32);
                assert_eq!(*q32_impl, BuiltinId::LpfxSaturateVec3Q32);
            }
            _ => panic!("Expected Decimal implementation"),
        }
    }

    #[test]
    fn test_find_lpfx_fn_by_builtin_id_q32() {
        use crate::backend::builtins::registry::BuiltinId;

        // Test that q32 builtin IDs also map to LPFX functions
        let q32_builtin = BuiltinId::LpfxSaturateVec3Q32;
        let lpfx_fn = find_lpfx_fn_by_builtin_id(q32_builtin);
        assert!(lpfx_fn.is_some());
        let lpfx_fn = lpfx_fn.unwrap();

        // Verify it's the correct function
        assert_eq!(lpfx_fn.glsl_sig.name, "lpfx_saturate");
    }

    #[test]
    fn test_find_lpfx_fn_by_builtin_id_non_lpfx() {
        use crate::backend::builtins::registry::BuiltinId;

        // Test that non-LPFX builtins return None
        let regular_builtin = BuiltinId::LpQ32Sin;
        let lpfx_fn = find_lpfx_fn_by_builtin_id(regular_builtin);
        assert!(
            lpfx_fn.is_none(),
            "Regular q32 builtin should not map to LPFX function"
        );
    }

    #[test]
    fn test_full_lookup_chain_f32_to_q32() {
        use crate::backend::builtins::registry::BuiltinId;

        // Test full lookup chain: name -> BuiltinId -> LpfxFn -> q32_impl -> name
        let f32_name = "__lpfx_saturate_vec3_f32";

        // Step 1: name -> BuiltinId
        let f32_builtin = BuiltinId::builtin_id_from_name(f32_name);
        assert_eq!(f32_builtin, Some(BuiltinId::LpfxSaturateVec3F32));

        // Step 2: BuiltinId -> LpfxFn
        let lpfx_fn = find_lpfx_fn_by_builtin_id(f32_builtin.unwrap());
        assert!(lpfx_fn.is_some());
        let lpfx_fn = lpfx_fn.unwrap();

        // Step 3: LpfxFn -> q32_impl
        match &lpfx_fn.impls {
            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::Decimal { q32_impl, .. } => {
                assert_eq!(*q32_impl, BuiltinId::LpfxSaturateVec3Q32);

                // Step 4: q32_impl -> name
                let q32_name = q32_impl.name();
                assert_eq!(q32_name, "__lpfx_saturate_vec3_q32");
            }
            _ => panic!("Expected Decimal implementation"),
        }
    }
}
