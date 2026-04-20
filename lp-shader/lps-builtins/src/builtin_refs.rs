//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

use crate::builtins::glsl::acos_q32::__lps_acos_q32;
use crate::builtins::glsl::acosh_q32::__lps_acosh_q32;
use crate::builtins::glsl::asin_q32::__lps_asin_q32;
use crate::builtins::glsl::asinh_q32::__lps_asinh_q32;
use crate::builtins::glsl::atan_q32::__lps_atan_q32;
use crate::builtins::glsl::atan2_q32::__lps_atan2_q32;
use crate::builtins::glsl::atanh_q32::__lps_atanh_q32;
use crate::builtins::glsl::cos_q32::__lps_cos_q32;
use crate::builtins::glsl::cosh_q32::__lps_cosh_q32;
use crate::builtins::glsl::exp_q32::__lps_exp_q32;
use crate::builtins::glsl::exp2_q32::__lps_exp2_q32;
use crate::builtins::glsl::fma_q32::__lps_fma_q32;
use crate::builtins::glsl::inversesqrt_q32::__lps_inversesqrt_q32;
use crate::builtins::glsl::ldexp_q32::__lps_ldexp_q32;
use crate::builtins::glsl::log_q32::__lps_log_q32;
use crate::builtins::glsl::log2_q32::__lps_log2_q32;
use crate::builtins::glsl::mod_q32::__lps_mod_q32;
use crate::builtins::glsl::pow_q32::__lps_pow_q32;
use crate::builtins::glsl::round_q32::__lps_round_q32;
use crate::builtins::glsl::sin_q32::__lps_sin_q32;
use crate::builtins::glsl::sincos_q32::__lps_sincos_q32;
use crate::builtins::glsl::sinh_q32::__lps_sinh_q32;
use crate::builtins::glsl::tan_q32::__lps_tan_q32;
use crate::builtins::glsl::tanh_q32::__lps_tanh_q32;
use crate::builtins::lpfn::color::space::hue2rgb_f32::__lp_lpfn_hue2rgb_f32;
use crate::builtins::lpfn::color::space::hue2rgb_q32::__lp_lpfn_hue2rgb_q32;
use crate::builtins::lpfn::color::space::{
    hsv2rgb_f32::__lp_lpfn_hsv2rgb_f32, hsv2rgb_f32::__lp_lpfn_hsv2rgb_vec4_f32,
};
use crate::builtins::lpfn::color::space::{
    hsv2rgb_q32::__lp_lpfn_hsv2rgb_q32, hsv2rgb_q32::__lp_lpfn_hsv2rgb_vec4_q32,
};
use crate::builtins::lpfn::color::space::{
    rgb2hsv_f32::__lp_lpfn_rgb2hsv_f32, rgb2hsv_f32::__lp_lpfn_rgb2hsv_vec4_f32,
};
use crate::builtins::lpfn::color::space::{
    rgb2hsv_q32::__lp_lpfn_rgb2hsv_q32, rgb2hsv_q32::__lp_lpfn_rgb2hsv_vec4_q32,
};
use crate::builtins::lpfn::generative::fbm::fbm2_f32::__lp_lpfn_fbm2_f32;
use crate::builtins::lpfn::generative::fbm::fbm2_q32::__lp_lpfn_fbm2_q32;
use crate::builtins::lpfn::generative::fbm::fbm3_f32::__lp_lpfn_fbm3_f32;
use crate::builtins::lpfn::generative::fbm::fbm3_q32::__lp_lpfn_fbm3_q32;
use crate::builtins::lpfn::generative::fbm::fbm3_tile_f32::__lp_lpfn_fbm3_tile_f32;
use crate::builtins::lpfn::generative::fbm::fbm3_tile_q32::__lp_lpfn_fbm3_tile_q32;
use crate::builtins::lpfn::generative::gnoise::gnoise1_f32::__lp_lpfn_gnoise1_f32;
use crate::builtins::lpfn::generative::gnoise::gnoise1_q32::__lp_lpfn_gnoise1_q32;
use crate::builtins::lpfn::generative::gnoise::gnoise2_f32::__lp_lpfn_gnoise2_f32;
use crate::builtins::lpfn::generative::gnoise::gnoise2_q32::__lp_lpfn_gnoise2_q32;
use crate::builtins::lpfn::generative::gnoise::gnoise3_f32::__lp_lpfn_gnoise3_f32;
use crate::builtins::lpfn::generative::gnoise::gnoise3_q32::__lp_lpfn_gnoise3_q32;
use crate::builtins::lpfn::generative::gnoise::gnoise3_tile_f32::__lp_lpfn_gnoise3_tile_f32;
use crate::builtins::lpfn::generative::gnoise::gnoise3_tile_q32::__lp_lpfn_gnoise3_tile_q32;
use crate::builtins::lpfn::generative::psrdnoise::psrdnoise2_f32::__lp_lpfn_psrdnoise2_f32;
use crate::builtins::lpfn::generative::psrdnoise::psrdnoise2_q32::__lp_lpfn_psrdnoise2_q32;
use crate::builtins::lpfn::generative::psrdnoise::psrdnoise3_f32::__lp_lpfn_psrdnoise3_f32;
use crate::builtins::lpfn::generative::psrdnoise::psrdnoise3_q32::__lp_lpfn_psrdnoise3_q32;
use crate::builtins::lpfn::generative::random::random1_f32::__lp_lpfn_random1_f32;
use crate::builtins::lpfn::generative::random::random1_q32::__lp_lpfn_random1_q32;
use crate::builtins::lpfn::generative::random::random2_f32::__lp_lpfn_random2_f32;
use crate::builtins::lpfn::generative::random::random2_q32::__lp_lpfn_random2_q32;
use crate::builtins::lpfn::generative::random::random3_f32::__lp_lpfn_random3_f32;
use crate::builtins::lpfn::generative::random::random3_q32::__lp_lpfn_random3_q32;
use crate::builtins::lpfn::generative::snoise::snoise1_f32::__lp_lpfn_snoise1_f32;
use crate::builtins::lpfn::generative::snoise::snoise1_q32::__lp_lpfn_snoise1_q32;
use crate::builtins::lpfn::generative::snoise::snoise2_f32::__lp_lpfn_snoise2_f32;
use crate::builtins::lpfn::generative::snoise::snoise2_q32::__lp_lpfn_snoise2_q32;
use crate::builtins::lpfn::generative::snoise::snoise3_f32::__lp_lpfn_snoise3_f32;
use crate::builtins::lpfn::generative::snoise::snoise3_q32::__lp_lpfn_snoise3_q32;
use crate::builtins::lpfn::generative::srandom::srandom1_f32::__lp_lpfn_srandom1_f32;
use crate::builtins::lpfn::generative::srandom::srandom1_q32::__lp_lpfn_srandom1_q32;
use crate::builtins::lpfn::generative::srandom::srandom2_f32::__lp_lpfn_srandom2_f32;
use crate::builtins::lpfn::generative::srandom::srandom2_q32::__lp_lpfn_srandom2_q32;
use crate::builtins::lpfn::generative::srandom::srandom3_f32::__lp_lpfn_srandom3_f32;
use crate::builtins::lpfn::generative::srandom::srandom3_q32::__lp_lpfn_srandom3_q32;
use crate::builtins::lpfn::generative::srandom::srandom3_tile_f32::__lp_lpfn_srandom3_tile_f32;
use crate::builtins::lpfn::generative::srandom::srandom3_tile_q32::__lp_lpfn_srandom3_tile_q32;
use crate::builtins::lpfn::generative::srandom::srandom3_vec_f32::__lp_lpfn_srandom3_vec_f32;
use crate::builtins::lpfn::generative::srandom::srandom3_vec_q32::__lp_lpfn_srandom3_vec_q32;
use crate::builtins::lpfn::generative::worley::worley2_f32::__lp_lpfn_worley2_f32;
use crate::builtins::lpfn::generative::worley::worley2_q32::__lp_lpfn_worley2_q32;
use crate::builtins::lpfn::generative::worley::worley2_value_f32::__lp_lpfn_worley2_value_f32;
use crate::builtins::lpfn::generative::worley::worley2_value_q32::__lp_lpfn_worley2_value_q32;
use crate::builtins::lpfn::generative::worley::worley3_f32::__lp_lpfn_worley3_f32;
use crate::builtins::lpfn::generative::worley::worley3_q32::__lp_lpfn_worley3_q32;
use crate::builtins::lpfn::generative::worley::worley3_value_f32::__lp_lpfn_worley3_value_f32;
use crate::builtins::lpfn::generative::worley::worley3_value_q32::__lp_lpfn_worley3_value_q32;
use crate::builtins::lpfn::math::{
    saturate_f32::__lp_lpfn_saturate_f32, saturate_f32::__lp_lpfn_saturate_vec3_f32,
    saturate_f32::__lp_lpfn_saturate_vec4_f32,
};
use crate::builtins::lpfn::math::{
    saturate_q32::__lp_lpfn_saturate_q32, saturate_q32::__lp_lpfn_saturate_vec3_q32,
    saturate_q32::__lp_lpfn_saturate_vec4_q32,
};
use crate::builtins::lpfn::{
    hash::__lp_lpfn_hash_1, hash::__lp_lpfn_hash_2, hash::__lp_lpfn_hash_3,
};
use crate::builtins::lpir::fadd_q32::__lp_lpir_fadd_q32;
use crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32;
use crate::builtins::lpir::fdiv_recip_q32::__lp_lpir_fdiv_recip_q32;
use crate::builtins::lpir::float_misc_q32::{
    __lp_lpir_fabs_q32, __lp_lpir_fceil_q32, __lp_lpir_ffloor_q32, __lp_lpir_fmax_q32,
    __lp_lpir_fmin_q32, __lp_lpir_ftrunc_q32,
};
use crate::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;
use crate::builtins::lpir::fnearest_q32::__lp_lpir_fnearest_q32;
use crate::builtins::lpir::fsqrt_q32::__lp_lpir_fsqrt_q32;
use crate::builtins::lpir::fsub_q32::__lp_lpir_fsub_q32;
use crate::builtins::lpir::ftoi_sat_q32::{__lp_lpir_ftoi_sat_s_q32, __lp_lpir_ftoi_sat_u_q32};
use crate::builtins::lpir::itof_s_q32::__lp_lpir_itof_s_q32;
use crate::builtins::lpir::itof_u_q32::__lp_lpir_itof_u_q32;
use crate::builtins::lpir::unorm_conv_q32::{
    __lp_lpir_fto_unorm8_q32, __lp_lpir_fto_unorm16_q32, __lp_lpir_unorm8_to_f_q32,
    __lp_lpir_unorm16_to_f_q32,
};
use crate::builtins::vm::get_fuel_q32::__lp_vm_get_fuel_q32;

/// Reference all builtin functions to prevent dead code elimination.
///
/// This function ensures all builtin functions are included in the executable
/// by creating function pointers and reading them with volatile operations.
pub fn ensure_builtins_referenced() {
    unsafe {
        let __lps_acos_q32_fn: extern "C" fn(i32) -> i32 = __lps_acos_q32;
        let __lps_acosh_q32_fn: extern "C" fn(i32) -> i32 = __lps_acosh_q32;
        let __lps_asin_q32_fn: extern "C" fn(i32) -> i32 = __lps_asin_q32;
        let __lps_asinh_q32_fn: extern "C" fn(i32) -> i32 = __lps_asinh_q32;
        let __lps_atan2_q32_fn: extern "C" fn(i32, i32) -> i32 = __lps_atan2_q32;
        let __lps_atan_q32_fn: extern "C" fn(i32) -> i32 = __lps_atan_q32;
        let __lps_atanh_q32_fn: extern "C" fn(i32) -> i32 = __lps_atanh_q32;
        let __lps_cos_q32_fn: extern "C" fn(i32) -> i32 = __lps_cos_q32;
        let __lps_cosh_q32_fn: extern "C" fn(i32) -> i32 = __lps_cosh_q32;
        let __lps_exp2_q32_fn: extern "C" fn(i32) -> i32 = __lps_exp2_q32;
        let __lps_exp_q32_fn: extern "C" fn(i32) -> i32 = __lps_exp_q32;
        let __lps_fma_q32_fn: extern "C" fn(i32, i32, i32) -> i32 = __lps_fma_q32;
        let __lps_inversesqrt_q32_fn: extern "C" fn(i32) -> i32 = __lps_inversesqrt_q32;
        let __lps_ldexp_q32_fn: extern "C" fn(i32, i32) -> i32 = __lps_ldexp_q32;
        let __lps_log2_q32_fn: extern "C" fn(i32) -> i32 = __lps_log2_q32;
        let __lps_log_q32_fn: extern "C" fn(i32) -> i32 = __lps_log_q32;
        let __lps_mod_q32_fn: extern "C" fn(i32, i32) -> i32 = __lps_mod_q32;
        let __lps_pow_q32_fn: extern "C" fn(i32, i32) -> i32 = __lps_pow_q32;
        let __lps_round_q32_fn: extern "C" fn(i32) -> i32 = __lps_round_q32;
        let __lps_sin_q32_fn: extern "C" fn(i32) -> i32 = __lps_sin_q32;
        let __lps_sincos_q32_fn: extern "C" fn(i32, *mut i32, *mut i32) -> () = __lps_sincos_q32;
        let __lps_sinh_q32_fn: extern "C" fn(i32) -> i32 = __lps_sinh_q32;
        let __lps_tan_q32_fn: extern "C" fn(i32) -> i32 = __lps_tan_q32;
        let __lps_tanh_q32_fn: extern "C" fn(i32) -> i32 = __lps_tanh_q32;
        let _lpir_fabs_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fabs_q32;
        let _lpir_fadd_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fadd_q32;
        let _lpir_fceil_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fceil_q32;
        let _lpir_fdiv_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fdiv_q32;
        let _lpir_fdiv_recip_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fdiv_recip_q32;
        let _lpir_ffloor_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_ffloor_q32;
        let _lpir_fmax_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fmax_q32;
        let _lpir_fmin_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fmin_q32;
        let _lpir_fmul_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fmul_q32;
        let _lpir_fnearest_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fnearest_q32;
        let _lpir_fsqrt_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fsqrt_q32;
        let _lpir_fsub_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fsub_q32;
        let _lpir_fto_unorm16_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fto_unorm16_q32;
        let _lpir_fto_unorm8_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fto_unorm8_q32;
        let _lpir_ftoi_sat_s_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_ftoi_sat_s_q32;
        let _lpir_ftoi_sat_u_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_ftoi_sat_u_q32;
        let _lpir_ftrunc_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_ftrunc_q32;
        let _lpir_itof_s_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_itof_s_q32;
        let _lpir_itof_u_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_itof_u_q32;
        let _lpir_unorm16_to_f_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_unorm16_to_f_q32;
        let _lpir_unorm8_to_f_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_unorm8_to_f_q32;
        let _lpfn_fbm2_f32_fn: extern "C" fn(f32, f32, i32, u32) -> f32 = __lp_lpfn_fbm2_f32;
        let _lpfn_fbm2_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfn_fbm2_q32;
        let _lpfn_fbm3_f32_fn: extern "C" fn(f32, f32, f32, i32, u32) -> f32 = __lp_lpfn_fbm3_f32;
        let _lpfn_fbm3_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 = __lp_lpfn_fbm3_q32;
        let _lpfn_fbm3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, i32, u32) -> f32 =
            __lp_lpfn_fbm3_tile_f32;
        let _lpfn_fbm3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, u32) -> i32 =
            __lp_lpfn_fbm3_tile_q32;
        let _lpfn_gnoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfn_gnoise1_f32;
        let _lpfn_gnoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfn_gnoise1_q32;
        let _lpfn_gnoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfn_gnoise2_f32;
        let _lpfn_gnoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfn_gnoise2_q32;
        let _lpfn_gnoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfn_gnoise3_f32;
        let _lpfn_gnoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfn_gnoise3_q32;
        let _lpfn_gnoise3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, u32) -> f32 =
            __lp_lpfn_gnoise3_tile_f32;
        let _lpfn_gnoise3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 =
            __lp_lpfn_gnoise3_tile_q32;
        let _lpfn_hash_1_fn: extern "C" fn(u32, u32) -> u32 = __lp_lpfn_hash_1;
        let _lpfn_hash_2_fn: extern "C" fn(u32, u32, u32) -> u32 = __lp_lpfn_hash_2;
        let _lpfn_hash_3_fn: extern "C" fn(u32, u32, u32, u32) -> u32 = __lp_lpfn_hash_3;
        let _lpfn_hsv2rgb_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfn_hsv2rgb_f32;
        let _lpfn_hsv2rgb_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfn_hsv2rgb_q32;
        let _lpfn_hsv2rgb_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfn_hsv2rgb_vec4_f32;
        let _lpfn_hsv2rgb_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfn_hsv2rgb_vec4_q32;
        let _lpfn_hue2rgb_f32_fn: extern "C" fn(*mut f32, f32) -> () = __lp_lpfn_hue2rgb_f32;
        let _lpfn_hue2rgb_q32_fn: extern "C" fn(*mut i32, i32) -> () = __lp_lpfn_hue2rgb_q32;
        let _lpfn_psrdnoise2_f32_fn: extern "C" fn(f32, f32, f32, f32, f32, *mut f32, u32) -> f32 =
            __lp_lpfn_psrdnoise2_f32;
        let _lpfn_psrdnoise2_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, *mut i32, u32) -> i32 =
            __lp_lpfn_psrdnoise2_q32;
        let _lpfn_psrdnoise3_f32_fn: extern "C" fn(
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            *mut f32,
            u32,
        ) -> f32 = __lp_lpfn_psrdnoise3_f32;
        let _lpfn_psrdnoise3_q32_fn: extern "C" fn(
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            *mut i32,
            u32,
        ) -> i32 = __lp_lpfn_psrdnoise3_q32;
        let _lpfn_random1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfn_random1_f32;
        let _lpfn_random1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfn_random1_q32;
        let _lpfn_random2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfn_random2_f32;
        let _lpfn_random2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfn_random2_q32;
        let _lpfn_random3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfn_random3_f32;
        let _lpfn_random3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfn_random3_q32;
        let _lpfn_rgb2hsv_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfn_rgb2hsv_f32;
        let _lpfn_rgb2hsv_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfn_rgb2hsv_q32;
        let _lpfn_rgb2hsv_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfn_rgb2hsv_vec4_f32;
        let _lpfn_rgb2hsv_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfn_rgb2hsv_vec4_q32;
        let _lpfn_saturate_f32_fn: extern "C" fn(f32) -> f32 = __lp_lpfn_saturate_f32;
        let _lpfn_saturate_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpfn_saturate_q32;
        let _lpfn_saturate_vec3_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfn_saturate_vec3_f32;
        let _lpfn_saturate_vec3_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfn_saturate_vec3_q32;
        let _lpfn_saturate_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfn_saturate_vec4_f32;
        let _lpfn_saturate_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfn_saturate_vec4_q32;
        let _lpfn_snoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfn_snoise1_f32;
        let _lpfn_snoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfn_snoise1_q32;
        let _lpfn_snoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfn_snoise2_f32;
        let _lpfn_snoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfn_snoise2_q32;
        let _lpfn_snoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfn_snoise3_f32;
        let _lpfn_snoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfn_snoise3_q32;
        let _lpfn_srandom1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfn_srandom1_f32;
        let _lpfn_srandom1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfn_srandom1_q32;
        let _lpfn_srandom2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfn_srandom2_f32;
        let _lpfn_srandom2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfn_srandom2_q32;
        let _lpfn_srandom3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 =
            __lp_lpfn_srandom3_f32;
        let _lpfn_srandom3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 =
            __lp_lpfn_srandom3_q32;
        let _lpfn_srandom3_tile_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32, u32) -> () =
            __lp_lpfn_srandom3_tile_f32;
        let _lpfn_srandom3_tile_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32, u32) -> () =
            __lp_lpfn_srandom3_tile_q32;
        let _lpfn_srandom3_vec_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, u32) -> () =
            __lp_lpfn_srandom3_vec_f32;
        let _lpfn_srandom3_vec_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, u32) -> () =
            __lp_lpfn_srandom3_vec_q32;
        let _lpfn_worley2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfn_worley2_f32;
        let _lpfn_worley2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfn_worley2_q32;
        let _lpfn_worley2_value_f32_fn: extern "C" fn(f32, f32, u32) -> f32 =
            __lp_lpfn_worley2_value_f32;
        let _lpfn_worley2_value_q32_fn: extern "C" fn(i32, i32, u32) -> i32 =
            __lp_lpfn_worley2_value_q32;
        let _lpfn_worley3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfn_worley3_f32;
        let _lpfn_worley3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfn_worley3_q32;
        let _lpfn_worley3_value_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 =
            __lp_lpfn_worley3_value_f32;
        let _lpfn_worley3_value_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 =
            __lp_lpfn_worley3_value_q32;
        let _vm_get_fuel_q32_fn: extern "C" fn(i32) -> u32 = __lp_vm_get_fuel_q32;

        // Force these to be included by using them in a way that can't be optimized away
        // We'll use volatile reads to prevent optimization
        let _ = core::ptr::read_volatile(&__lps_acos_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_acosh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_asin_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_asinh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_atan2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_atan_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_atanh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_cos_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_cosh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_exp2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_exp_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_fma_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_inversesqrt_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_ldexp_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_log2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_log_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_mod_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_pow_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_round_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_sin_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_sincos_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_sinh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_tan_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lps_tanh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fabs_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fadd_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fceil_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fdiv_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fdiv_recip_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_ffloor_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fmax_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fmin_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fmul_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fnearest_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fsqrt_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fsub_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fto_unorm16_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fto_unorm8_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_ftoi_sat_s_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_ftoi_sat_u_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_ftrunc_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_itof_s_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_itof_u_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_unorm16_to_f_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_unorm8_to_f_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_fbm3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_gnoise3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hash_1_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hash_2_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hash_3_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hsv2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hsv2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hsv2rgb_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hsv2rgb_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hue2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_hue2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_psrdnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_psrdnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_psrdnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_psrdnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_random3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_rgb2hsv_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_rgb2hsv_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_rgb2hsv_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_rgb2hsv_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_vec3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_vec3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_saturate_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_snoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_vec_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_srandom3_vec_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley2_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley2_value_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley3_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfn_worley3_value_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_vm_get_fuel_q32_fn as *const _);
    }
}
