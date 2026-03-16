//! Map float libcall names to Q32 BuiltinIds.
//!
//! This mapping is used by direct Q32 codegen when dispatching math builtins.
//!
//! This function is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this function, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

use super::registry::BuiltinId;

/// Map TestCase function name and argument count to BuiltinId.
///
/// Returns None if the function name is not a math function that should be converted.
/// Handles both standard C math function names (sinf, cosf) and intrinsic names (__lp_sin, __lp_cos).
/// Supports overloaded functions by matching on both name and argument count.
///
/// This function is AUTO-GENERATED. Do not edit manually.
///
/// To regenerate this function, run:
///     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
///
/// Or use the build script:
///     scripts/build-builtins.sh
pub fn map_testcase_to_builtin(testcase_name: &str, arg_count: usize) -> Option<BuiltinId> {
    match (testcase_name, arg_count) {
        ("lp_q32_acosf" | "__lp_q32_acos" | "acosf", 1) => Some(BuiltinId::LpQ32Acos),
        ("lp_q32_acoshf" | "__lp_q32_acosh" | "acoshf", 1) => Some(BuiltinId::LpQ32Acosh),
        ("lp_q32_addf" | "__lp_q32_add" | "addf", 2) => Some(BuiltinId::LpQ32Add),
        ("lp_q32_asinf" | "__lp_q32_asin" | "asinf", 1) => Some(BuiltinId::LpQ32Asin),
        ("lp_q32_asinhf" | "__lp_q32_asinh" | "asinhf", 1) => Some(BuiltinId::LpQ32Asinh),
        ("lp_q32_atanf" | "__lp_q32_atan" | "atanf", 1) => Some(BuiltinId::LpQ32Atan),
        ("lp_q32_atan2f" | "__lp_q32_atan2" | "atan2f", 2) => Some(BuiltinId::LpQ32Atan2),
        ("lp_q32_atanhf" | "__lp_q32_atanh" | "atanhf", 1) => Some(BuiltinId::LpQ32Atanh),
        ("lp_q32_cosf" | "__lp_q32_cos" | "cosf", 1) => Some(BuiltinId::LpQ32Cos),
        ("lp_q32_coshf" | "__lp_q32_cosh" | "coshf", 1) => Some(BuiltinId::LpQ32Cosh),
        ("lp_q32_divf" | "__lp_q32_div" | "divf", 2) => Some(BuiltinId::LpQ32Div),
        ("lp_q32_expf" | "__lp_q32_exp" | "expf", 1) => Some(BuiltinId::LpQ32Exp),
        ("lp_q32_exp2f" | "__lp_q32_exp2" | "exp2f", 1) => Some(BuiltinId::LpQ32Exp2),
        ("lp_q32_fmaf" | "__lp_q32_fma" | "fmaf", 3) => Some(BuiltinId::LpQ32Fma),
        ("lp_q32_inversesqrtf" | "__lp_q32_inversesqrt" | "inversesqrtf", 1) => {
            Some(BuiltinId::LpQ32Inversesqrt)
        }
        ("lp_q32_ldexpf" | "__lp_q32_ldexp" | "ldexpf", 2) => Some(BuiltinId::LpQ32Ldexp),
        ("lp_q32_logf" | "__lp_q32_log" | "logf", 1) => Some(BuiltinId::LpQ32Log),
        ("lp_q32_log2f" | "__lp_q32_log2" | "log2f", 1) => Some(BuiltinId::LpQ32Log2),
        ("lp_q32_modf" | "__lp_q32_mod" | "fmodf", 2) => Some(BuiltinId::LpQ32Mod),
        ("lp_q32_mulf" | "__lp_q32_mul" | "mulf", 2) => Some(BuiltinId::LpQ32Mul),
        ("lp_q32_powf" | "__lp_q32_pow" | "powf", 2) => Some(BuiltinId::LpQ32Pow),
        ("lp_q32_roundf" | "__lp_q32_round" | "roundf", 1) => Some(BuiltinId::LpQ32Round),
        ("lp_q32_roundevenf" | "__lp_q32_roundeven" | "roundevenf", 1) => {
            Some(BuiltinId::LpQ32Roundeven)
        }
        ("lp_q32_sinf" | "__lp_q32_sin" | "sinf", 1) => Some(BuiltinId::LpQ32Sin),
        ("lp_q32_sinhf" | "__lp_q32_sinh" | "sinhf", 1) => Some(BuiltinId::LpQ32Sinh),
        ("lp_q32_sqrtf" | "__lp_q32_sqrt" | "sqrtf", 1) => Some(BuiltinId::LpQ32Sqrt),
        ("lp_q32_subf" | "__lp_q32_sub" | "subf", 2) => Some(BuiltinId::LpQ32Sub),
        ("lp_q32_tanf" | "__lp_q32_tan" | "tanf", 1) => Some(BuiltinId::LpQ32Tan),
        ("lp_q32_tanhf" | "__lp_q32_tanh" | "tanhf", 1) => Some(BuiltinId::LpQ32Tanh),
        _ => None,
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::map_testcase_to_builtin;
    use crate::backend::builtins::BuiltinId;

    #[test]
    fn test_map_testcase_to_builtin_simplex() {
        // LPFX functions are no longer handled by map_testcase_to_builtin
        assert_eq!(map_testcase_to_builtin("__lpfx_snoise1", 2), None);
        assert_eq!(map_testcase_to_builtin("__lpfx_snoise2", 3), None);
        assert_eq!(map_testcase_to_builtin("__lpfx_snoise3", 4), None);
    }

    #[test]
    fn test_map_testcase_to_builtin_hash() {
        assert_eq!(map_testcase_to_builtin("lpfx_hash_1f", 2), None);
        assert_eq!(map_testcase_to_builtin("__lp_lpfx_hash_1", 2), None);
        assert_eq!(map_testcase_to_builtin("lpfx_hash_2f", 3), None);
        assert_eq!(map_testcase_to_builtin("__lp_lpfx_hash_2", 3), None);
        assert_eq!(map_testcase_to_builtin("lpfx_hash_3f", 4), None);
        assert_eq!(map_testcase_to_builtin("__lp_lpfx_hash_3", 4), None);
    }

    #[test]
    fn test_map_testcase_to_builtin_standard_math() {
        assert_eq!(
            map_testcase_to_builtin("lp_q32_sinf", 1),
            Some(BuiltinId::LpQ32Sin)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_q32_sin", 1),
            Some(BuiltinId::LpQ32Sin)
        );
        assert_eq!(
            map_testcase_to_builtin("lp_q32_addf", 2),
            Some(BuiltinId::LpQ32Add)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_q32_add", 2),
            Some(BuiltinId::LpQ32Add)
        );
        assert_eq!(
            map_testcase_to_builtin("lp_q32_fmaf", 3),
            Some(BuiltinId::LpQ32Fma)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_q32_fma", 3),
            Some(BuiltinId::LpQ32Fma)
        );
    }

    #[test]
    fn test_map_testcase_to_builtin_overloads() {
        assert_eq!(map_testcase_to_builtin("__lpfx_hsv2rgb", 4), None);
        assert_eq!(map_testcase_to_builtin("__lpfx_hsv2rgb", 5), None);
        assert_eq!(map_testcase_to_builtin("__lpfx_hsv2rgb", 3), None);
        assert_eq!(map_testcase_to_builtin("__lpfx_hsv2rgb", 6), None);
    }

    #[test]
    fn test_map_testcase_to_builtin_unknown() {
        assert_eq!(map_testcase_to_builtin("unknown_function", 0), None);
        assert_eq!(map_testcase_to_builtin("__lp_unknown", 0), None);
        assert_eq!(map_testcase_to_builtin("", 0), None);
    }
}
