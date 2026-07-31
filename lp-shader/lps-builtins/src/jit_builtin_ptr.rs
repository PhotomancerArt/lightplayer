//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! Machine code pointers for builtins (JIT / native link).
//!
//! The native-f32 family is behind the `float-f32` feature. With the feature
//! off those ids have no implementation linked, and asking for one is a
//! programming error in the caller (it compiled an f32 module against a
//! Fixed-only builtin image), so the lookup aborts rather than handing back a
//! null pointer for the JIT to call.

use lps_builtin_ids::BuiltinId;

/// Address of the `extern "C"` implementation for `builtin` (for auipc+jalr relocation targets).
#[must_use]
pub fn jit_builtin_code_ptr(builtin: BuiltinId) -> *const u8 {
    match builtin {
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAcosF32 => crate::builtins::glsl::acos_f32::__lps_acos_f32 as *const u8,
        BuiltinId::LpGlslAcosQ32 => crate::builtins::glsl::acos_q32::__lps_acos_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAcoshF32 => crate::builtins::glsl::acosh_f32::__lps_acosh_f32 as *const u8,
        BuiltinId::LpGlslAcoshQ32 => crate::builtins::glsl::acosh_q32::__lps_acosh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAsinF32 => crate::builtins::glsl::asin_f32::__lps_asin_f32 as *const u8,
        BuiltinId::LpGlslAsinQ32 => crate::builtins::glsl::asin_q32::__lps_asin_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAsinhF32 => crate::builtins::glsl::asinh_f32::__lps_asinh_f32 as *const u8,
        BuiltinId::LpGlslAsinhQ32 => crate::builtins::glsl::asinh_q32::__lps_asinh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAtan2F32 => crate::builtins::glsl::atan2_f32::__lps_atan2_f32 as *const u8,
        BuiltinId::LpGlslAtan2Q32 => crate::builtins::glsl::atan2_q32::__lps_atan2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAtanF32 => crate::builtins::glsl::atan_f32::__lps_atan_f32 as *const u8,
        BuiltinId::LpGlslAtanQ32 => crate::builtins::glsl::atan_q32::__lps_atan_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslAtanhF32 => crate::builtins::glsl::atanh_f32::__lps_atanh_f32 as *const u8,
        BuiltinId::LpGlslAtanhQ32 => crate::builtins::glsl::atanh_q32::__lps_atanh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslCosF32 => crate::builtins::glsl::cos_f32::__lps_cos_f32 as *const u8,
        BuiltinId::LpGlslCosQ32 => crate::builtins::glsl::cos_q32::__lps_cos_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslCoshF32 => crate::builtins::glsl::cosh_f32::__lps_cosh_f32 as *const u8,
        BuiltinId::LpGlslCoshQ32 => crate::builtins::glsl::cosh_q32::__lps_cosh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslExp2F32 => crate::builtins::glsl::exp2_f32::__lps_exp2_f32 as *const u8,
        BuiltinId::LpGlslExp2Q32 => crate::builtins::glsl::exp2_q32::__lps_exp2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslExpF32 => crate::builtins::glsl::exp_f32::__lps_exp_f32 as *const u8,
        BuiltinId::LpGlslExpQ32 => crate::builtins::glsl::exp_q32::__lps_exp_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslFmaF32 => crate::builtins::glsl::fma_f32::__lps_fma_f32 as *const u8,
        BuiltinId::LpGlslFmaQ32 => crate::builtins::glsl::fma_q32::__lps_fma_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslInversesqrtF32 => crate::builtins::glsl::inversesqrt_f32::__lps_inversesqrt_f32 as *const u8,
        BuiltinId::LpGlslInversesqrtQ32 => crate::builtins::glsl::inversesqrt_q32::__lps_inversesqrt_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslLdexpF32 => crate::builtins::glsl::ldexp_f32::__lps_ldexp_f32 as *const u8,
        BuiltinId::LpGlslLdexpQ32 => crate::builtins::glsl::ldexp_q32::__lps_ldexp_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslLog2F32 => crate::builtins::glsl::log2_f32::__lps_log2_f32 as *const u8,
        BuiltinId::LpGlslLog2Q32 => crate::builtins::glsl::log2_q32::__lps_log2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslLogF32 => crate::builtins::glsl::log_f32::__lps_log_f32 as *const u8,
        BuiltinId::LpGlslLogQ32 => crate::builtins::glsl::log_q32::__lps_log_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslModF32 => crate::builtins::glsl::mod_f32::__lps_mod_f32 as *const u8,
        BuiltinId::LpGlslModQ32 => crate::builtins::glsl::mod_q32::__lps_mod_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslPowF32 => crate::builtins::glsl::pow_f32::__lps_pow_f32 as *const u8,
        BuiltinId::LpGlslPowQ32 => crate::builtins::glsl::pow_q32::__lps_pow_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslRoundF32 => crate::builtins::glsl::round_f32::__lps_round_f32 as *const u8,
        BuiltinId::LpGlslRoundQ32 => crate::builtins::glsl::round_q32::__lps_round_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslSinF32 => crate::builtins::glsl::sin_f32::__lps_sin_f32 as *const u8,
        BuiltinId::LpGlslSinQ32 => crate::builtins::glsl::sin_q32::__lps_sin_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslSincosF32 => crate::builtins::glsl::sincos_f32::__lps_sincos_f32 as *const u8,
        BuiltinId::LpGlslSincosQ32 => crate::builtins::glsl::sincos_q32::__lps_sincos_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslSinhF32 => crate::builtins::glsl::sinh_f32::__lps_sinh_f32 as *const u8,
        BuiltinId::LpGlslSinhQ32 => crate::builtins::glsl::sinh_q32::__lps_sinh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslTanF32 => crate::builtins::glsl::tan_f32::__lps_tan_f32 as *const u8,
        BuiltinId::LpGlslTanQ32 => crate::builtins::glsl::tan_q32::__lps_tan_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpGlslTanhF32 => crate::builtins::glsl::tanh_f32::__lps_tanh_f32 as *const u8,
        BuiltinId::LpGlslTanhQ32 => crate::builtins::glsl::tanh_q32::__lps_tanh_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFabsF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_fabs_f32 as *const u8,
        BuiltinId::LpLpirFabsQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_fabs_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFaddF32 => crate::builtins::lpir::fadd_f32::__lp_lpir_fadd_f32 as *const u8,
        BuiltinId::LpLpirFaddQ32 => crate::builtins::lpir::fadd_q32::__lp_lpir_fadd_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFceilF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_fceil_f32 as *const u8,
        BuiltinId::LpLpirFceilQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_fceil_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFdivF32 => crate::builtins::lpir::fdiv_f32::__lp_lpir_fdiv_f32 as *const u8,
        BuiltinId::LpLpirFdivQ32 => crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFdivRecipF32 => crate::builtins::lpir::fdiv_recip_f32::__lp_lpir_fdiv_recip_f32 as *const u8,
        BuiltinId::LpLpirFdivRecipQ32 => crate::builtins::lpir::fdiv_recip_q32::__lp_lpir_fdiv_recip_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFfloorF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_ffloor_f32 as *const u8,
        BuiltinId::LpLpirFfloorQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_ffloor_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFmaxF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_fmax_f32 as *const u8,
        BuiltinId::LpLpirFmaxQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_fmax_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFminF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_fmin_f32 as *const u8,
        BuiltinId::LpLpirFminQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_fmin_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFmulF32 => crate::builtins::lpir::fmul_f32::__lp_lpir_fmul_f32 as *const u8,
        BuiltinId::LpLpirFmulQ32 => crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFnearestF32 => crate::builtins::lpir::fnearest_f32::__lp_lpir_fnearest_f32 as *const u8,
        BuiltinId::LpLpirFnearestQ32 => crate::builtins::lpir::fnearest_q32::__lp_lpir_fnearest_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFsqrtF32 => crate::builtins::lpir::fsqrt_f32::__lp_lpir_fsqrt_f32 as *const u8,
        BuiltinId::LpLpirFsqrtQ32 => crate::builtins::lpir::fsqrt_q32::__lp_lpir_fsqrt_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFsubF32 => crate::builtins::lpir::fsub_f32::__lp_lpir_fsub_f32 as *const u8,
        BuiltinId::LpLpirFsubQ32 => crate::builtins::lpir::fsub_q32::__lp_lpir_fsub_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFtoUnorm16F32 => crate::builtins::lpir::unorm_conv_f32::__lp_lpir_fto_unorm16_f32 as *const u8,
        BuiltinId::LpLpirFtoUnorm16Q32 => crate::builtins::lpir::unorm_conv_q32::__lp_lpir_fto_unorm16_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFtoUnorm8F32 => crate::builtins::lpir::unorm_conv_f32::__lp_lpir_fto_unorm8_f32 as *const u8,
        BuiltinId::LpLpirFtoUnorm8Q32 => crate::builtins::lpir::unorm_conv_q32::__lp_lpir_fto_unorm8_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFtoiSatSF32 => crate::builtins::lpir::ftoi_sat_f32::__lp_lpir_ftoi_sat_s_f32 as *const u8,
        BuiltinId::LpLpirFtoiSatSQ32 => crate::builtins::lpir::ftoi_sat_q32::__lp_lpir_ftoi_sat_s_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFtoiSatUF32 => crate::builtins::lpir::ftoi_sat_f32::__lp_lpir_ftoi_sat_u_f32 as *const u8,
        BuiltinId::LpLpirFtoiSatUQ32 => crate::builtins::lpir::ftoi_sat_q32::__lp_lpir_ftoi_sat_u_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirFtruncF32 => crate::builtins::lpir::float_misc_f32::__lp_lpir_ftrunc_f32 as *const u8,
        BuiltinId::LpLpirFtruncQ32 => crate::builtins::lpir::float_misc_q32::__lp_lpir_ftrunc_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirItofSF32 => crate::builtins::lpir::itof_s_f32::__lp_lpir_itof_s_f32 as *const u8,
        BuiltinId::LpLpirItofSQ32 => crate::builtins::lpir::itof_s_q32::__lp_lpir_itof_s_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirItofUF32 => crate::builtins::lpir::itof_u_f32::__lp_lpir_itof_u_f32 as *const u8,
        BuiltinId::LpLpirItofUQ32 => crate::builtins::lpir::itof_u_q32::__lp_lpir_itof_u_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirUnorm16ToFF32 => crate::builtins::lpir::unorm_conv_f32::__lp_lpir_unorm16_to_f_f32 as *const u8,
        BuiltinId::LpLpirUnorm16ToFQ32 => crate::builtins::lpir::unorm_conv_q32::__lp_lpir_unorm16_to_f_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpirUnorm8ToFF32 => crate::builtins::lpir::unorm_conv_f32::__lp_lpir_unorm8_to_f_f32 as *const u8,
        BuiltinId::LpLpirUnorm8ToFQ32 => crate::builtins::lpir::unorm_conv_q32::__lp_lpir_unorm8_to_f_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnFbm2F32 => crate::builtins::lpfn::generative::fbm::fbm2_f32::__lp_lpfn_fbm2_f32 as *const u8,
        BuiltinId::LpLpfnFbm2Q32 => crate::builtins::lpfn::generative::fbm::fbm2_q32::__lp_lpfn_fbm2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnFbm3F32 => crate::builtins::lpfn::generative::fbm::fbm3_f32::__lp_lpfn_fbm3_f32 as *const u8,
        BuiltinId::LpLpfnFbm3Q32 => crate::builtins::lpfn::generative::fbm::fbm3_q32::__lp_lpfn_fbm3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnFbm3TileF32 => crate::builtins::lpfn::generative::fbm::fbm3_tile_f32::__lp_lpfn_fbm3_tile_f32 as *const u8,
        BuiltinId::LpLpfnFbm3TileQ32 => crate::builtins::lpfn::generative::fbm::fbm3_tile_q32::__lp_lpfn_fbm3_tile_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnGnoise1F32 => crate::builtins::lpfn::generative::gnoise::gnoise1_f32::__lp_lpfn_gnoise1_f32 as *const u8,
        BuiltinId::LpLpfnGnoise1Q32 => crate::builtins::lpfn::generative::gnoise::gnoise1_q32::__lp_lpfn_gnoise1_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnGnoise2F32 => crate::builtins::lpfn::generative::gnoise::gnoise2_f32::__lp_lpfn_gnoise2_f32 as *const u8,
        BuiltinId::LpLpfnGnoise2Q32 => crate::builtins::lpfn::generative::gnoise::gnoise2_q32::__lp_lpfn_gnoise2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnGnoise3F32 => crate::builtins::lpfn::generative::gnoise::gnoise3_f32::__lp_lpfn_gnoise3_f32 as *const u8,
        BuiltinId::LpLpfnGnoise3Q32 => crate::builtins::lpfn::generative::gnoise::gnoise3_q32::__lp_lpfn_gnoise3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnGnoise3TileF32 => crate::builtins::lpfn::generative::gnoise::gnoise3_tile_f32::__lp_lpfn_gnoise3_tile_f32 as *const u8,
        BuiltinId::LpLpfnGnoise3TileQ32 => crate::builtins::lpfn::generative::gnoise::gnoise3_tile_q32::__lp_lpfn_gnoise3_tile_q32 as *const u8,
        BuiltinId::LpLpfnHash1 => crate::builtins::lpfn::hash::__lp_lpfn_hash_1 as *const u8,
        BuiltinId::LpLpfnHash2 => crate::builtins::lpfn::hash::__lp_lpfn_hash_2 as *const u8,
        BuiltinId::LpLpfnHash3 => crate::builtins::lpfn::hash::__lp_lpfn_hash_3 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnHsv2rgbF32 => crate::builtins::lpfn::color::space::hsv2rgb_f32::__lp_lpfn_hsv2rgb_f32 as *const u8,
        BuiltinId::LpLpfnHsv2rgbQ32 => crate::builtins::lpfn::color::space::hsv2rgb_q32::__lp_lpfn_hsv2rgb_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnHsv2rgbVec4F32 => crate::builtins::lpfn::color::space::hsv2rgb_f32::__lp_lpfn_hsv2rgb_vec4_f32 as *const u8,
        BuiltinId::LpLpfnHsv2rgbVec4Q32 => crate::builtins::lpfn::color::space::hsv2rgb_q32::__lp_lpfn_hsv2rgb_vec4_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnHue2rgbF32 => crate::builtins::lpfn::color::space::hue2rgb_f32::__lp_lpfn_hue2rgb_f32 as *const u8,
        BuiltinId::LpLpfnHue2rgbQ32 => crate::builtins::lpfn::color::space::hue2rgb_q32::__lp_lpfn_hue2rgb_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnPsrdnoise2F32 => crate::builtins::lpfn::generative::psrdnoise::psrdnoise2_f32::__lp_lpfn_psrdnoise2_f32 as *const u8,
        BuiltinId::LpLpfnPsrdnoise2Q32 => crate::builtins::lpfn::generative::psrdnoise::psrdnoise2_q32::__lp_lpfn_psrdnoise2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnPsrdnoise3F32 => crate::builtins::lpfn::generative::psrdnoise::psrdnoise3_f32::__lp_lpfn_psrdnoise3_f32 as *const u8,
        BuiltinId::LpLpfnPsrdnoise3Q32 => crate::builtins::lpfn::generative::psrdnoise::psrdnoise3_q32::__lp_lpfn_psrdnoise3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnRandom1F32 => crate::builtins::lpfn::generative::random::random1_f32::__lp_lpfn_random1_f32 as *const u8,
        BuiltinId::LpLpfnRandom1Q32 => crate::builtins::lpfn::generative::random::random1_q32::__lp_lpfn_random1_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnRandom2F32 => crate::builtins::lpfn::generative::random::random2_f32::__lp_lpfn_random2_f32 as *const u8,
        BuiltinId::LpLpfnRandom2Q32 => crate::builtins::lpfn::generative::random::random2_q32::__lp_lpfn_random2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnRandom3F32 => crate::builtins::lpfn::generative::random::random3_f32::__lp_lpfn_random3_f32 as *const u8,
        BuiltinId::LpLpfnRandom3Q32 => crate::builtins::lpfn::generative::random::random3_q32::__lp_lpfn_random3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnRgb2hsvF32 => crate::builtins::lpfn::color::space::rgb2hsv_f32::__lp_lpfn_rgb2hsv_f32 as *const u8,
        BuiltinId::LpLpfnRgb2hsvQ32 => crate::builtins::lpfn::color::space::rgb2hsv_q32::__lp_lpfn_rgb2hsv_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnRgb2hsvVec4F32 => crate::builtins::lpfn::color::space::rgb2hsv_f32::__lp_lpfn_rgb2hsv_vec4_f32 as *const u8,
        BuiltinId::LpLpfnRgb2hsvVec4Q32 => crate::builtins::lpfn::color::space::rgb2hsv_q32::__lp_lpfn_rgb2hsv_vec4_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSaturateF32 => crate::builtins::lpfn::math::saturate_f32::__lp_lpfn_saturate_f32 as *const u8,
        BuiltinId::LpLpfnSaturateQ32 => crate::builtins::lpfn::math::saturate_q32::__lp_lpfn_saturate_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSaturateVec3F32 => crate::builtins::lpfn::math::saturate_f32::__lp_lpfn_saturate_vec3_f32 as *const u8,
        BuiltinId::LpLpfnSaturateVec3Q32 => crate::builtins::lpfn::math::saturate_q32::__lp_lpfn_saturate_vec3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSaturateVec4F32 => crate::builtins::lpfn::math::saturate_f32::__lp_lpfn_saturate_vec4_f32 as *const u8,
        BuiltinId::LpLpfnSaturateVec4Q32 => crate::builtins::lpfn::math::saturate_q32::__lp_lpfn_saturate_vec4_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSnoise1F32 => crate::builtins::lpfn::generative::snoise::snoise1_f32::__lp_lpfn_snoise1_f32 as *const u8,
        BuiltinId::LpLpfnSnoise1Q32 => crate::builtins::lpfn::generative::snoise::snoise1_q32::__lp_lpfn_snoise1_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSnoise2F32 => crate::builtins::lpfn::generative::snoise::snoise2_f32::__lp_lpfn_snoise2_f32 as *const u8,
        BuiltinId::LpLpfnSnoise2Q32 => crate::builtins::lpfn::generative::snoise::snoise2_q32::__lp_lpfn_snoise2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSnoise3F32 => crate::builtins::lpfn::generative::snoise::snoise3_f32::__lp_lpfn_snoise3_f32 as *const u8,
        BuiltinId::LpLpfnSnoise3Q32 => crate::builtins::lpfn::generative::snoise::snoise3_q32::__lp_lpfn_snoise3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSrandom1F32 => crate::builtins::lpfn::generative::srandom::srandom1_f32::__lp_lpfn_srandom1_f32 as *const u8,
        BuiltinId::LpLpfnSrandom1Q32 => crate::builtins::lpfn::generative::srandom::srandom1_q32::__lp_lpfn_srandom1_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSrandom2F32 => crate::builtins::lpfn::generative::srandom::srandom2_f32::__lp_lpfn_srandom2_f32 as *const u8,
        BuiltinId::LpLpfnSrandom2Q32 => crate::builtins::lpfn::generative::srandom::srandom2_q32::__lp_lpfn_srandom2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSrandom3F32 => crate::builtins::lpfn::generative::srandom::srandom3_f32::__lp_lpfn_srandom3_f32 as *const u8,
        BuiltinId::LpLpfnSrandom3Q32 => crate::builtins::lpfn::generative::srandom::srandom3_q32::__lp_lpfn_srandom3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSrandom3TileF32 => crate::builtins::lpfn::generative::srandom::srandom3_tile_f32::__lp_lpfn_srandom3_tile_f32 as *const u8,
        BuiltinId::LpLpfnSrandom3TileQ32 => crate::builtins::lpfn::generative::srandom::srandom3_tile_q32::__lp_lpfn_srandom3_tile_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnSrandom3VecF32 => crate::builtins::lpfn::generative::srandom::srandom3_vec_f32::__lp_lpfn_srandom3_vec_f32 as *const u8,
        BuiltinId::LpLpfnSrandom3VecQ32 => crate::builtins::lpfn::generative::srandom::srandom3_vec_q32::__lp_lpfn_srandom3_vec_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnWorley2F32 => crate::builtins::lpfn::generative::worley::worley2_f32::__lp_lpfn_worley2_f32 as *const u8,
        BuiltinId::LpLpfnWorley2Q32 => crate::builtins::lpfn::generative::worley::worley2_q32::__lp_lpfn_worley2_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnWorley2ValueF32 => crate::builtins::lpfn::generative::worley::worley2_value_f32::__lp_lpfn_worley2_value_f32 as *const u8,
        BuiltinId::LpLpfnWorley2ValueQ32 => crate::builtins::lpfn::generative::worley::worley2_value_q32::__lp_lpfn_worley2_value_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnWorley3F32 => crate::builtins::lpfn::generative::worley::worley3_f32::__lp_lpfn_worley3_f32 as *const u8,
        BuiltinId::LpLpfnWorley3Q32 => crate::builtins::lpfn::generative::worley::worley3_q32::__lp_lpfn_worley3_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpLpfnWorley3ValueF32 => crate::builtins::lpfn::generative::worley::worley3_value_f32::__lp_lpfn_worley3_value_f32 as *const u8,
        BuiltinId::LpLpfnWorley3ValueQ32 => crate::builtins::lpfn::generative::worley::worley3_value_q32::__lp_lpfn_worley3_value_q32 as *const u8,
        BuiltinId::LpVmGetFuel => crate::builtins::vm::get_fuel::__lp_vm_get_fuel as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpTexTexture1dR16UnormF32 => crate::builtins::texture::r16_unorm_f32::__lp_texture1d_r16_unorm_f32 as *const u8,
        BuiltinId::LpTexTexture1dR16UnormQ32 => crate::builtins::texture::r16_unorm_q32::__lp_texture1d_r16_unorm_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpTexTexture1dRgba16UnormF32 => crate::builtins::texture::rgba16_unorm_f32::__lp_texture1d_rgba16_unorm_f32 as *const u8,
        BuiltinId::LpTexTexture1dRgba16UnormQ32 => crate::builtins::texture::rgba16_unorm_q32::__lp_texture1d_rgba16_unorm_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpTexTexture2dR16UnormF32 => crate::builtins::texture::r16_unorm_f32::__lp_texture2d_r16_unorm_f32 as *const u8,
        BuiltinId::LpTexTexture2dR16UnormQ32 => crate::builtins::texture::r16_unorm_q32::__lp_texture2d_r16_unorm_q32 as *const u8,
        #[cfg(feature = "float-f32")]
        BuiltinId::LpTexTexture2dRgba16UnormF32 => crate::builtins::texture::rgba16_unorm_f32::__lp_texture2d_rgba16_unorm_f32 as *const u8,
        BuiltinId::LpTexTexture2dRgba16UnormQ32 => crate::builtins::texture::rgba16_unorm_q32::__lp_texture2d_rgba16_unorm_q32 as *const u8,
        #[cfg(not(feature = "float-f32"))]
        _ => f32_family_not_linked(),
    }
}

/// A native-f32 builtin was requested from an image built without `float-f32`.
#[cfg(not(feature = "float-f32"))]
#[cold]
#[inline(never)]
fn f32_family_not_linked() -> ! {
    panic!("native-f32 builtin requested but the `float-f32` feature is off")
}
