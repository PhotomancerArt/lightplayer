//! Machine code pointers for builtins (JIT / native link).
//!
//! Keep in sync with `lpvm_cranelift::generated_builtin_abi::get_function_pointer_inner` whenever
//! builtins are added or renamed (same `BuiltinId` arms and symbol names).

use lps_builtin_ids::BuiltinId;

/// Address of the `extern "C"` implementation for `builtin` (for auipc+jalr relocation targets).
#[must_use]
pub fn jit_builtin_code_ptr(builtin: BuiltinId) -> *const u8 {
    use crate::builtins::{
        glsl::{
            acos_q32, acosh_q32, asin_q32, asinh_q32, atan_q32, atan2_q32, atanh_q32, cos_q32,
            cosh_q32, exp_q32, exp2_q32, fma_q32, inversesqrt_q32, ldexp_q32, log_q32, log2_q32,
            mod_q32, pow_q32, round_q32, sin_q32, sinh_q32, tan_q32, tanh_q32,
        },
        lpfn::{color, generative, hash, math},
        lpir::{
            fadd_q32, fdiv_q32, fdiv_recip_q32, float_misc_q32, fmul_q32, fnearest_q32, fsqrt_q32,
            fsub_q32, ftoi_sat_q32, itof_s_q32, itof_u_q32, unorm_conv_q32,
        },
        vm::get_fuel_q32,
    };
    match builtin {
        BuiltinId::LpGlslAcosQ32 => acos_q32::__lps_acos_q32 as *const u8,
        BuiltinId::LpGlslAcoshQ32 => acosh_q32::__lps_acosh_q32 as *const u8,
        BuiltinId::LpGlslAsinQ32 => asin_q32::__lps_asin_q32 as *const u8,
        BuiltinId::LpGlslAsinhQ32 => asinh_q32::__lps_asinh_q32 as *const u8,
        BuiltinId::LpGlslAtan2Q32 => atan2_q32::__lps_atan2_q32 as *const u8,
        BuiltinId::LpGlslAtanQ32 => atan_q32::__lps_atan_q32 as *const u8,
        BuiltinId::LpGlslAtanhQ32 => atanh_q32::__lps_atanh_q32 as *const u8,
        BuiltinId::LpGlslCosQ32 => cos_q32::__lps_cos_q32 as *const u8,
        BuiltinId::LpGlslCoshQ32 => cosh_q32::__lps_cosh_q32 as *const u8,
        BuiltinId::LpGlslExp2Q32 => exp2_q32::__lps_exp2_q32 as *const u8,
        BuiltinId::LpGlslExpQ32 => exp_q32::__lps_exp_q32 as *const u8,
        BuiltinId::LpGlslFmaQ32 => fma_q32::__lps_fma_q32 as *const u8,
        BuiltinId::LpGlslInversesqrtQ32 => inversesqrt_q32::__lps_inversesqrt_q32 as *const u8,
        BuiltinId::LpGlslLdexpQ32 => ldexp_q32::__lps_ldexp_q32 as *const u8,
        BuiltinId::LpGlslLog2Q32 => log2_q32::__lps_log2_q32 as *const u8,
        BuiltinId::LpGlslLogQ32 => log_q32::__lps_log_q32 as *const u8,
        BuiltinId::LpGlslModQ32 => mod_q32::__lps_mod_q32 as *const u8,
        BuiltinId::LpGlslPowQ32 => pow_q32::__lps_pow_q32 as *const u8,
        BuiltinId::LpGlslRoundQ32 => round_q32::__lps_round_q32 as *const u8,
        BuiltinId::LpGlslSinQ32 => sin_q32::__lps_sin_q32 as *const u8,
        BuiltinId::LpGlslSinhQ32 => sinh_q32::__lps_sinh_q32 as *const u8,
        BuiltinId::LpGlslTanQ32 => tan_q32::__lps_tan_q32 as *const u8,
        BuiltinId::LpGlslTanhQ32 => tanh_q32::__lps_tanh_q32 as *const u8,
        BuiltinId::LpLpirFabsQ32 => float_misc_q32::__lp_lpir_fabs_q32 as *const u8,
        BuiltinId::LpLpirFaddQ32 => fadd_q32::__lp_lpir_fadd_q32 as *const u8,
        BuiltinId::LpLpirFceilQ32 => float_misc_q32::__lp_lpir_fceil_q32 as *const u8,
        BuiltinId::LpLpirFdivQ32 => fdiv_q32::__lp_lpir_fdiv_q32 as *const u8,
        BuiltinId::LpLpirFdivRecipQ32 => fdiv_recip_q32::__lp_lpir_fdiv_recip_q32 as *const u8,
        BuiltinId::LpLpirFfloorQ32 => float_misc_q32::__lp_lpir_ffloor_q32 as *const u8,
        BuiltinId::LpLpirFmaxQ32 => float_misc_q32::__lp_lpir_fmax_q32 as *const u8,
        BuiltinId::LpLpirFminQ32 => float_misc_q32::__lp_lpir_fmin_q32 as *const u8,
        BuiltinId::LpLpirFmulQ32 => fmul_q32::__lp_lpir_fmul_q32 as *const u8,
        BuiltinId::LpLpirFnearestQ32 => fnearest_q32::__lp_lpir_fnearest_q32 as *const u8,
        BuiltinId::LpLpirFsqrtQ32 => fsqrt_q32::__lp_lpir_fsqrt_q32 as *const u8,
        BuiltinId::LpLpirFsubQ32 => fsub_q32::__lp_lpir_fsub_q32 as *const u8,
        BuiltinId::LpLpirFtoiSatSQ32 => ftoi_sat_q32::__lp_lpir_ftoi_sat_s_q32 as *const u8,
        BuiltinId::LpLpirFtoiSatUQ32 => ftoi_sat_q32::__lp_lpir_ftoi_sat_u_q32 as *const u8,
        BuiltinId::LpLpirFtoUnorm16Q32 => unorm_conv_q32::__lp_lpir_fto_unorm16_q32 as *const u8,
        BuiltinId::LpLpirFtoUnorm8Q32 => unorm_conv_q32::__lp_lpir_fto_unorm8_q32 as *const u8,
        BuiltinId::LpLpirFtruncQ32 => float_misc_q32::__lp_lpir_ftrunc_q32 as *const u8,
        BuiltinId::LpLpirItofSQ32 => itof_s_q32::__lp_lpir_itof_s_q32 as *const u8,
        BuiltinId::LpLpirItofUQ32 => itof_u_q32::__lp_lpir_itof_u_q32 as *const u8,
        BuiltinId::LpLpirUnorm16ToFQ32 => unorm_conv_q32::__lp_lpir_unorm16_to_f_q32 as *const u8,
        BuiltinId::LpLpirUnorm8ToFQ32 => unorm_conv_q32::__lp_lpir_unorm8_to_f_q32 as *const u8,
        BuiltinId::LpLpfnFbm2F32 => generative::fbm::fbm2_f32::__lp_lpfn_fbm2_f32 as *const u8,
        BuiltinId::LpLpfnFbm2Q32 => generative::fbm::fbm2_q32::__lp_lpfn_fbm2_q32 as *const u8,
        BuiltinId::LpLpfnFbm3F32 => generative::fbm::fbm3_f32::__lp_lpfn_fbm3_f32 as *const u8,
        BuiltinId::LpLpfnFbm3Q32 => generative::fbm::fbm3_q32::__lp_lpfn_fbm3_q32 as *const u8,
        BuiltinId::LpLpfnFbm3TileF32 => {
            generative::fbm::fbm3_tile_f32::__lp_lpfn_fbm3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfnFbm3TileQ32 => {
            generative::fbm::fbm3_tile_q32::__lp_lpfn_fbm3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfnGnoise1F32 => {
            generative::gnoise::gnoise1_f32::__lp_lpfn_gnoise1_f32 as *const u8
        }
        BuiltinId::LpLpfnGnoise1Q32 => {
            generative::gnoise::gnoise1_q32::__lp_lpfn_gnoise1_q32 as *const u8
        }
        BuiltinId::LpLpfnGnoise2F32 => {
            generative::gnoise::gnoise2_f32::__lp_lpfn_gnoise2_f32 as *const u8
        }
        BuiltinId::LpLpfnGnoise2Q32 => {
            generative::gnoise::gnoise2_q32::__lp_lpfn_gnoise2_q32 as *const u8
        }
        BuiltinId::LpLpfnGnoise3F32 => {
            generative::gnoise::gnoise3_f32::__lp_lpfn_gnoise3_f32 as *const u8
        }
        BuiltinId::LpLpfnGnoise3Q32 => {
            generative::gnoise::gnoise3_q32::__lp_lpfn_gnoise3_q32 as *const u8
        }
        BuiltinId::LpLpfnGnoise3TileF32 => {
            generative::gnoise::gnoise3_tile_f32::__lp_lpfn_gnoise3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfnGnoise3TileQ32 => {
            generative::gnoise::gnoise3_tile_q32::__lp_lpfn_gnoise3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfnHash1 => hash::__lp_lpfn_hash_1 as *const u8,
        BuiltinId::LpLpfnHash2 => hash::__lp_lpfn_hash_2 as *const u8,
        BuiltinId::LpLpfnHash3 => hash::__lp_lpfn_hash_3 as *const u8,
        BuiltinId::LpLpfnHsv2rgbF32 => {
            color::space::hsv2rgb_f32::__lp_lpfn_hsv2rgb_f32 as *const u8
        }
        BuiltinId::LpLpfnHsv2rgbQ32 => {
            color::space::hsv2rgb_q32::__lp_lpfn_hsv2rgb_q32 as *const u8
        }
        BuiltinId::LpLpfnHsv2rgbVec4F32 => {
            color::space::hsv2rgb_f32::__lp_lpfn_hsv2rgb_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfnHsv2rgbVec4Q32 => {
            color::space::hsv2rgb_q32::__lp_lpfn_hsv2rgb_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfnHue2rgbF32 => {
            color::space::hue2rgb_f32::__lp_lpfn_hue2rgb_f32 as *const u8
        }
        BuiltinId::LpLpfnHue2rgbQ32 => {
            color::space::hue2rgb_q32::__lp_lpfn_hue2rgb_q32 as *const u8
        }
        BuiltinId::LpLpfnPsrdnoise2F32 => {
            generative::psrdnoise::psrdnoise2_f32::__lp_lpfn_psrdnoise2_f32 as *const u8
        }
        BuiltinId::LpLpfnPsrdnoise2Q32 => {
            generative::psrdnoise::psrdnoise2_q32::__lp_lpfn_psrdnoise2_q32 as *const u8
        }
        BuiltinId::LpLpfnPsrdnoise3F32 => {
            generative::psrdnoise::psrdnoise3_f32::__lp_lpfn_psrdnoise3_f32 as *const u8
        }
        BuiltinId::LpLpfnPsrdnoise3Q32 => {
            generative::psrdnoise::psrdnoise3_q32::__lp_lpfn_psrdnoise3_q32 as *const u8
        }
        BuiltinId::LpLpfnRandom1F32 => {
            generative::random::random1_f32::__lp_lpfn_random1_f32 as *const u8
        }
        BuiltinId::LpLpfnRandom1Q32 => {
            generative::random::random1_q32::__lp_lpfn_random1_q32 as *const u8
        }
        BuiltinId::LpLpfnRandom2F32 => {
            generative::random::random2_f32::__lp_lpfn_random2_f32 as *const u8
        }
        BuiltinId::LpLpfnRandom2Q32 => {
            generative::random::random2_q32::__lp_lpfn_random2_q32 as *const u8
        }
        BuiltinId::LpLpfnRandom3F32 => {
            generative::random::random3_f32::__lp_lpfn_random3_f32 as *const u8
        }
        BuiltinId::LpLpfnRandom3Q32 => {
            generative::random::random3_q32::__lp_lpfn_random3_q32 as *const u8
        }
        BuiltinId::LpLpfnRgb2hsvF32 => {
            color::space::rgb2hsv_f32::__lp_lpfn_rgb2hsv_f32 as *const u8
        }
        BuiltinId::LpLpfnRgb2hsvQ32 => {
            color::space::rgb2hsv_q32::__lp_lpfn_rgb2hsv_q32 as *const u8
        }
        BuiltinId::LpLpfnRgb2hsvVec4F32 => {
            color::space::rgb2hsv_f32::__lp_lpfn_rgb2hsv_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfnRgb2hsvVec4Q32 => {
            color::space::rgb2hsv_q32::__lp_lpfn_rgb2hsv_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfnSaturateF32 => math::saturate_f32::__lp_lpfn_saturate_f32 as *const u8,
        BuiltinId::LpLpfnSaturateQ32 => math::saturate_q32::__lp_lpfn_saturate_q32 as *const u8,
        BuiltinId::LpLpfnSaturateVec3F32 => {
            math::saturate_f32::__lp_lpfn_saturate_vec3_f32 as *const u8
        }
        BuiltinId::LpLpfnSaturateVec3Q32 => {
            math::saturate_q32::__lp_lpfn_saturate_vec3_q32 as *const u8
        }
        BuiltinId::LpLpfnSaturateVec4F32 => {
            math::saturate_f32::__lp_lpfn_saturate_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfnSaturateVec4Q32 => {
            math::saturate_q32::__lp_lpfn_saturate_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfnSnoise1F32 => {
            generative::snoise::snoise1_f32::__lp_lpfn_snoise1_f32 as *const u8
        }
        BuiltinId::LpLpfnSnoise1Q32 => {
            generative::snoise::snoise1_q32::__lp_lpfn_snoise1_q32 as *const u8
        }
        BuiltinId::LpLpfnSnoise2F32 => {
            generative::snoise::snoise2_f32::__lp_lpfn_snoise2_f32 as *const u8
        }
        BuiltinId::LpLpfnSnoise2Q32 => {
            generative::snoise::snoise2_q32::__lp_lpfn_snoise2_q32 as *const u8
        }
        BuiltinId::LpLpfnSnoise3F32 => {
            generative::snoise::snoise3_f32::__lp_lpfn_snoise3_f32 as *const u8
        }
        BuiltinId::LpLpfnSnoise3Q32 => {
            generative::snoise::snoise3_q32::__lp_lpfn_snoise3_q32 as *const u8
        }
        BuiltinId::LpLpfnSrandom1F32 => {
            generative::srandom::srandom1_f32::__lp_lpfn_srandom1_f32 as *const u8
        }
        BuiltinId::LpLpfnSrandom1Q32 => {
            generative::srandom::srandom1_q32::__lp_lpfn_srandom1_q32 as *const u8
        }
        BuiltinId::LpLpfnSrandom2F32 => {
            generative::srandom::srandom2_f32::__lp_lpfn_srandom2_f32 as *const u8
        }
        BuiltinId::LpLpfnSrandom2Q32 => {
            generative::srandom::srandom2_q32::__lp_lpfn_srandom2_q32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3F32 => {
            generative::srandom::srandom3_f32::__lp_lpfn_srandom3_f32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3Q32 => {
            generative::srandom::srandom3_q32::__lp_lpfn_srandom3_q32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3TileF32 => {
            generative::srandom::srandom3_tile_f32::__lp_lpfn_srandom3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3TileQ32 => {
            generative::srandom::srandom3_tile_q32::__lp_lpfn_srandom3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3VecF32 => {
            generative::srandom::srandom3_vec_f32::__lp_lpfn_srandom3_vec_f32 as *const u8
        }
        BuiltinId::LpLpfnSrandom3VecQ32 => {
            generative::srandom::srandom3_vec_q32::__lp_lpfn_srandom3_vec_q32 as *const u8
        }
        BuiltinId::LpLpfnWorley2F32 => {
            generative::worley::worley2_f32::__lp_lpfn_worley2_f32 as *const u8
        }
        BuiltinId::LpLpfnWorley2Q32 => {
            generative::worley::worley2_q32::__lp_lpfn_worley2_q32 as *const u8
        }
        BuiltinId::LpLpfnWorley2ValueF32 => {
            generative::worley::worley2_value_f32::__lp_lpfn_worley2_value_f32 as *const u8
        }
        BuiltinId::LpLpfnWorley2ValueQ32 => {
            generative::worley::worley2_value_q32::__lp_lpfn_worley2_value_q32 as *const u8
        }
        BuiltinId::LpLpfnWorley3F32 => {
            generative::worley::worley3_f32::__lp_lpfn_worley3_f32 as *const u8
        }
        BuiltinId::LpLpfnWorley3Q32 => {
            generative::worley::worley3_q32::__lp_lpfn_worley3_q32 as *const u8
        }
        BuiltinId::LpLpfnWorley3ValueF32 => {
            generative::worley::worley3_value_f32::__lp_lpfn_worley3_value_f32 as *const u8
        }
        BuiltinId::LpLpfnWorley3ValueQ32 => {
            generative::worley::worley3_value_q32::__lp_lpfn_worley3_value_q32 as *const u8
        }
        BuiltinId::LpVmGetFuelQ32 => get_fuel_q32::__lp_vm_get_fuel_q32 as *const u8,
    }
}
