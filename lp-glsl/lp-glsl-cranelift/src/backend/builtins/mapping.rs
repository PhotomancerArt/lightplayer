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
        ("lp_glsl_acosf" | "__lp_glsl_acos_q32" | "acosf", 1) => Some(BuiltinId::LpGlslAcosQ32),
        ("lp_glsl_acoshf" | "__lp_glsl_acosh_q32" | "acoshf", 1) => Some(BuiltinId::LpGlslAcoshQ32),
        ("lp_glsl_asinf" | "__lp_glsl_asin_q32" | "asinf", 1) => Some(BuiltinId::LpGlslAsinQ32),
        ("lp_glsl_asinhf" | "__lp_glsl_asinh_q32" | "asinhf", 1) => Some(BuiltinId::LpGlslAsinhQ32),
        ("lp_glsl_atan2f" | "__lp_glsl_atan2_q32" | "atan2f", 2) => Some(BuiltinId::LpGlslAtan2Q32),
        ("lp_glsl_atanf" | "__lp_glsl_atan_q32" | "atanf", 1) => Some(BuiltinId::LpGlslAtanQ32),
        ("lp_glsl_atanhf" | "__lp_glsl_atanh_q32" | "atanhf", 1) => Some(BuiltinId::LpGlslAtanhQ32),
        ("lp_glsl_cosf" | "__lp_glsl_cos_q32" | "cosf", 1) => Some(BuiltinId::LpGlslCosQ32),
        ("lp_glsl_coshf" | "__lp_glsl_cosh_q32" | "coshf", 1) => Some(BuiltinId::LpGlslCoshQ32),
        ("lp_glsl_exp2f" | "__lp_glsl_exp2_q32" | "exp2f", 1) => Some(BuiltinId::LpGlslExp2Q32),
        ("lp_glsl_expf" | "__lp_glsl_exp_q32" | "expf", 1) => Some(BuiltinId::LpGlslExpQ32),
        ("lp_glsl_fmaf" | "__lp_glsl_fma_q32" | "fmaf", 3) => Some(BuiltinId::LpGlslFmaQ32),
        ("lp_glsl_inversesqrtf" | "__lp_glsl_inversesqrt_q32" | "inversesqrtf", 1) => {
            Some(BuiltinId::LpGlslInversesqrtQ32)
        }
        ("lp_glsl_ldexpf" | "__lp_glsl_ldexp_q32" | "ldexpf", 2) => Some(BuiltinId::LpGlslLdexpQ32),
        ("lp_glsl_log2f" | "__lp_glsl_log2_q32" | "log2f", 1) => Some(BuiltinId::LpGlslLog2Q32),
        ("lp_glsl_logf" | "__lp_glsl_log_q32" | "logf", 1) => Some(BuiltinId::LpGlslLogQ32),
        ("lp_glsl_modf" | "__lp_glsl_mod_q32" | "fmodf", 2) => Some(BuiltinId::LpGlslModQ32),
        ("lp_glsl_powf" | "__lp_glsl_pow_q32" | "powf", 2) => Some(BuiltinId::LpGlslPowQ32),
        ("lp_glsl_roundf" | "__lp_glsl_round_q32" | "roundf", 1) => Some(BuiltinId::LpGlslRoundQ32),
        ("lp_glsl_sinf" | "__lp_glsl_sin_q32" | "sinf", 1) => Some(BuiltinId::LpGlslSinQ32),
        ("lp_glsl_sinhf" | "__lp_glsl_sinh_q32" | "sinhf", 1) => Some(BuiltinId::LpGlslSinhQ32),
        ("lp_glsl_tanf" | "__lp_glsl_tan_q32" | "tanf", 1) => Some(BuiltinId::LpGlslTanQ32),
        ("lp_glsl_tanhf" | "__lp_glsl_tanh_q32" | "tanhf", 1) => Some(BuiltinId::LpGlslTanhQ32),
        ("lp_lpir_faddf" | "__lp_lpir_fadd_q32", 2) => Some(BuiltinId::LpLpirFaddQ32),
        ("lp_lpir_fdivf" | "__lp_lpir_fdiv_q32", 2) => Some(BuiltinId::LpLpirFdivQ32),
        ("lp_lpir_fmulf" | "__lp_lpir_fmul_q32", 2) => Some(BuiltinId::LpLpirFmulQ32),
        ("lp_lpir_fnearestf" | "__lp_lpir_fnearest_q32" | "roundevenf", 1) => {
            Some(BuiltinId::LpLpirFnearestQ32)
        }
        ("lp_lpir_fsqrtf" | "__lp_lpir_fsqrt_q32" | "sqrtf", 1) => Some(BuiltinId::LpLpirFsqrtQ32),
        ("lp_lpir_fsubf" | "__lp_lpir_fsub_q32", 2) => Some(BuiltinId::LpLpirFsubQ32),
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
            map_testcase_to_builtin("lp_glsl_sinf", 1),
            Some(BuiltinId::LpGlslSinQ32)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_glsl_sin_q32", 1),
            Some(BuiltinId::LpGlslSinQ32)
        );
        assert_eq!(
            map_testcase_to_builtin("lp_lpir_faddf", 2),
            Some(BuiltinId::LpLpirFaddQ32)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_lpir_fadd_q32", 2),
            Some(BuiltinId::LpLpirFaddQ32)
        );
        assert_eq!(
            map_testcase_to_builtin("lp_glsl_fmaf", 3),
            Some(BuiltinId::LpGlslFmaQ32)
        );
        assert_eq!(
            map_testcase_to_builtin("__lp_glsl_fma_q32", 3),
            Some(BuiltinId::LpGlslFmaQ32)
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
