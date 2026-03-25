//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

use lp_glsl_builtins::builtins::lpfx::color::space::hue2rgb_f32::__lp_lpfx_hue2rgb_f32;
use lp_glsl_builtins::builtins::lpfx::color::space::hue2rgb_q32::__lp_lpfx_hue2rgb_q32;
use lp_glsl_builtins::builtins::lpfx::color::space::{
    hsv2rgb_f32::__lp_lpfx_hsv2rgb_f32, hsv2rgb_f32::__lp_lpfx_hsv2rgb_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    hsv2rgb_q32::__lp_lpfx_hsv2rgb_q32, hsv2rgb_q32::__lp_lpfx_hsv2rgb_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    rgb2hsv_f32::__lp_lpfx_rgb2hsv_f32, rgb2hsv_f32::__lp_lpfx_rgb2hsv_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    rgb2hsv_q32::__lp_lpfx_rgb2hsv_q32, rgb2hsv_q32::__lp_lpfx_rgb2hsv_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm2_f32::__lp_lpfx_fbm2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm2_q32::__lp_lpfx_fbm2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_f32::__lp_lpfx_fbm3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_q32::__lp_lpfx_fbm3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_tile_f32::__lp_lpfx_fbm3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_tile_q32::__lp_lpfx_fbm3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise1_f32::__lp_lpfx_gnoise1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise1_q32::__lp_lpfx_gnoise1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise2_f32::__lp_lpfx_gnoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise2_q32::__lp_lpfx_gnoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_f32::__lp_lpfx_gnoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_q32::__lp_lpfx_gnoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_tile_f32::__lp_lpfx_gnoise3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_tile_q32::__lp_lpfx_gnoise3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise2_f32::__lp_lpfx_psrdnoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise2_q32::__lp_lpfx_psrdnoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise3_f32::__lp_lpfx_psrdnoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise3_q32::__lp_lpfx_psrdnoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random1_f32::__lp_lpfx_random1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random1_q32::__lp_lpfx_random1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random2_f32::__lp_lpfx_random2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random2_q32::__lp_lpfx_random2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random3_f32::__lp_lpfx_random3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random3_q32::__lp_lpfx_random3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise1_f32::__lp_lpfx_snoise1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise1_q32::__lp_lpfx_snoise1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise2_f32::__lp_lpfx_snoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise2_q32::__lp_lpfx_snoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise3_f32::__lp_lpfx_snoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise3_q32::__lp_lpfx_snoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom1_f32::__lp_lpfx_srandom1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom1_q32::__lp_lpfx_srandom1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom2_f32::__lp_lpfx_srandom2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom2_q32::__lp_lpfx_srandom2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_f32::__lp_lpfx_srandom3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_q32::__lp_lpfx_srandom3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_tile_f32::__lp_lpfx_srandom3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_tile_q32::__lp_lpfx_srandom3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_vec_f32::__lp_lpfx_srandom3_vec_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_vec_q32::__lp_lpfx_srandom3_vec_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_f32::__lp_lpfx_worley2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_q32::__lp_lpfx_worley2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_value_f32::__lp_lpfx_worley2_value_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_value_q32::__lp_lpfx_worley2_value_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_f32::__lp_lpfx_worley3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_q32::__lp_lpfx_worley3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_value_f32::__lp_lpfx_worley3_value_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_value_q32::__lp_lpfx_worley3_value_q32;
use lp_glsl_builtins::builtins::lpfx::math::{
    saturate_f32::__lp_lpfx_saturate_f32, saturate_f32::__lp_lpfx_saturate_vec3_f32,
    saturate_f32::__lp_lpfx_saturate_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::math::{
    saturate_q32::__lp_lpfx_saturate_q32, saturate_q32::__lp_lpfx_saturate_vec3_q32,
    saturate_q32::__lp_lpfx_saturate_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::{
    hash::__lp_lpfx_hash_1, hash::__lp_lpfx_hash_2, hash::__lp_lpfx_hash_3,
};
use lp_glsl_builtins::builtins::q32::__lp_glsl_acos_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_acosh_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_asin_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_asinh_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_atan_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_atan2_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_atanh_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_cos_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_cosh_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_exp_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_exp2_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_fma_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_inversesqrt_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_ldexp_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_log_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_log2_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_mod_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_pow_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_round_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_sin_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_sinh_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_tan_q32;
use lp_glsl_builtins::builtins::q32::__lp_glsl_tanh_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fadd_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fdiv_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fmul_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fnearest_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fsqrt_q32;
use lp_glsl_builtins::builtins::q32::__lp_lpir_fsub_q32;

/// Reference all builtin functions to prevent dead code elimination.
///
/// This function ensures all builtin functions are included in the executable
/// by creating function pointers and reading them with volatile operations.
pub fn ensure_builtins_referenced() {
    unsafe {
        let _glsl_acos_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_acos_q32;
        let _glsl_acosh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_acosh_q32;
        let _glsl_asin_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_asin_q32;
        let _glsl_asinh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_asinh_q32;
        let _glsl_atan2_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_glsl_atan2_q32;
        let _glsl_atan_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_atan_q32;
        let _glsl_atanh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_atanh_q32;
        let _glsl_cos_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_cos_q32;
        let _glsl_cosh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_cosh_q32;
        let _glsl_exp2_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_exp2_q32;
        let _glsl_exp_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_exp_q32;
        let _glsl_fma_q32_fn: extern "C" fn(i32, i32, i32) -> i32 = __lp_glsl_fma_q32;
        let _glsl_inversesqrt_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_inversesqrt_q32;
        let _glsl_ldexp_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_glsl_ldexp_q32;
        let _glsl_log2_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_log2_q32;
        let _glsl_log_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_log_q32;
        let _glsl_mod_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_glsl_mod_q32;
        let _glsl_pow_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_glsl_pow_q32;
        let _glsl_round_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_round_q32;
        let _glsl_sin_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_sin_q32;
        let _glsl_sinh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_sinh_q32;
        let _glsl_tan_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_tan_q32;
        let _glsl_tanh_q32_fn: extern "C" fn(i32) -> i32 = __lp_glsl_tanh_q32;
        let _lpir_fadd_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fadd_q32;
        let _lpir_fdiv_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fdiv_q32;
        let _lpir_fmul_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fmul_q32;
        let _lpir_fnearest_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fnearest_q32;
        let _lpir_fsqrt_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpir_fsqrt_q32;
        let _lpir_fsub_q32_fn: extern "C" fn(i32, i32) -> i32 = __lp_lpir_fsub_q32;
        let _lpfx_fbm2_f32_fn: extern "C" fn(f32, f32, i32, u32) -> f32 = __lp_lpfx_fbm2_f32;
        let _lpfx_fbm2_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfx_fbm2_q32;
        let _lpfx_fbm3_f32_fn: extern "C" fn(f32, f32, f32, i32, u32) -> f32 = __lp_lpfx_fbm3_f32;
        let _lpfx_fbm3_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 = __lp_lpfx_fbm3_q32;
        let _lpfx_fbm3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, i32, u32) -> f32 =
            __lp_lpfx_fbm3_tile_f32;
        let _lpfx_fbm3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, u32) -> i32 =
            __lp_lpfx_fbm3_tile_q32;
        let _lpfx_gnoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfx_gnoise1_f32;
        let _lpfx_gnoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfx_gnoise1_q32;
        let _lpfx_gnoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfx_gnoise2_f32;
        let _lpfx_gnoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfx_gnoise2_q32;
        let _lpfx_gnoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfx_gnoise3_f32;
        let _lpfx_gnoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfx_gnoise3_q32;
        let _lpfx_gnoise3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, u32) -> f32 =
            __lp_lpfx_gnoise3_tile_f32;
        let _lpfx_gnoise3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 =
            __lp_lpfx_gnoise3_tile_q32;
        let _lpfx_hash_1_fn: extern "C" fn(u32, u32) -> u32 = __lp_lpfx_hash_1;
        let _lpfx_hash_2_fn: extern "C" fn(u32, u32, u32) -> u32 = __lp_lpfx_hash_2;
        let _lpfx_hash_3_fn: extern "C" fn(u32, u32, u32, u32) -> u32 = __lp_lpfx_hash_3;
        let _lpfx_hsv2rgb_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfx_hsv2rgb_f32;
        let _lpfx_hsv2rgb_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfx_hsv2rgb_q32;
        let _lpfx_hsv2rgb_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfx_hsv2rgb_vec4_f32;
        let _lpfx_hsv2rgb_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfx_hsv2rgb_vec4_q32;
        let _lpfx_hue2rgb_f32_fn: extern "C" fn(*mut f32, f32) -> () = __lp_lpfx_hue2rgb_f32;
        let _lpfx_hue2rgb_q32_fn: extern "C" fn(*mut i32, i32) -> () = __lp_lpfx_hue2rgb_q32;
        let _lpfx_psrdnoise2_f32_fn: extern "C" fn(f32, f32, f32, f32, f32, *mut f32, u32) -> f32 =
            __lp_lpfx_psrdnoise2_f32;
        let _lpfx_psrdnoise2_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, *mut i32, u32) -> i32 =
            __lp_lpfx_psrdnoise2_q32;
        let _lpfx_psrdnoise3_f32_fn: extern "C" fn(
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            *mut f32,
            u32,
        ) -> f32 = __lp_lpfx_psrdnoise3_f32;
        let _lpfx_psrdnoise3_q32_fn: extern "C" fn(
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            *mut i32,
            u32,
        ) -> i32 = __lp_lpfx_psrdnoise3_q32;
        let _lpfx_random1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfx_random1_f32;
        let _lpfx_random1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfx_random1_q32;
        let _lpfx_random2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfx_random2_f32;
        let _lpfx_random2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfx_random2_q32;
        let _lpfx_random3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfx_random3_f32;
        let _lpfx_random3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfx_random3_q32;
        let _lpfx_rgb2hsv_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfx_rgb2hsv_f32;
        let _lpfx_rgb2hsv_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfx_rgb2hsv_q32;
        let _lpfx_rgb2hsv_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfx_rgb2hsv_vec4_f32;
        let _lpfx_rgb2hsv_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfx_rgb2hsv_vec4_q32;
        let _lpfx_saturate_f32_fn: extern "C" fn(f32) -> f32 = __lp_lpfx_saturate_f32;
        let _lpfx_saturate_q32_fn: extern "C" fn(i32) -> i32 = __lp_lpfx_saturate_q32;
        let _lpfx_saturate_vec3_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lp_lpfx_saturate_vec3_f32;
        let _lpfx_saturate_vec3_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lp_lpfx_saturate_vec3_q32;
        let _lpfx_saturate_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lp_lpfx_saturate_vec4_f32;
        let _lpfx_saturate_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lp_lpfx_saturate_vec4_q32;
        let _lpfx_snoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfx_snoise1_f32;
        let _lpfx_snoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfx_snoise1_q32;
        let _lpfx_snoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfx_snoise2_f32;
        let _lpfx_snoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfx_snoise2_q32;
        let _lpfx_snoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfx_snoise3_f32;
        let _lpfx_snoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfx_snoise3_q32;
        let _lpfx_srandom1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lp_lpfx_srandom1_f32;
        let _lpfx_srandom1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lp_lpfx_srandom1_q32;
        let _lpfx_srandom2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfx_srandom2_f32;
        let _lpfx_srandom2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfx_srandom2_q32;
        let _lpfx_srandom3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 =
            __lp_lpfx_srandom3_f32;
        let _lpfx_srandom3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 =
            __lp_lpfx_srandom3_q32;
        let _lpfx_srandom3_tile_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32, u32) -> () =
            __lp_lpfx_srandom3_tile_f32;
        let _lpfx_srandom3_tile_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32, u32) -> () =
            __lp_lpfx_srandom3_tile_q32;
        let _lpfx_srandom3_vec_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, u32) -> () =
            __lp_lpfx_srandom3_vec_f32;
        let _lpfx_srandom3_vec_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, u32) -> () =
            __lp_lpfx_srandom3_vec_q32;
        let _lpfx_worley2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lp_lpfx_worley2_f32;
        let _lpfx_worley2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lp_lpfx_worley2_q32;
        let _lpfx_worley2_value_f32_fn: extern "C" fn(f32, f32, u32) -> f32 =
            __lp_lpfx_worley2_value_f32;
        let _lpfx_worley2_value_q32_fn: extern "C" fn(i32, i32, u32) -> i32 =
            __lp_lpfx_worley2_value_q32;
        let _lpfx_worley3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lp_lpfx_worley3_f32;
        let _lpfx_worley3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lp_lpfx_worley3_q32;
        let _lpfx_worley3_value_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 =
            __lp_lpfx_worley3_value_f32;
        let _lpfx_worley3_value_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 =
            __lp_lpfx_worley3_value_q32;

        // Force these to be included by using them in a way that can't be optimized away
        // We'll use volatile reads to prevent optimization
        let _ = core::ptr::read_volatile(&_glsl_acos_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_acosh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_asin_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_asinh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_atan2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_atan_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_atanh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_cos_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_cosh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_exp2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_exp_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_fma_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_inversesqrt_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_ldexp_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_log2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_log_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_mod_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_pow_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_round_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_sin_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_sinh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_tan_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_glsl_tanh_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fadd_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fdiv_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fmul_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fnearest_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fsqrt_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpir_fsub_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_fbm3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_gnoise3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hash_1_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hash_2_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hash_3_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hsv2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hsv2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hsv2rgb_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hsv2rgb_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hue2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_hue2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_psrdnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_psrdnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_psrdnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_psrdnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_random3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_rgb2hsv_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_rgb2hsv_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_rgb2hsv_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_rgb2hsv_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_vec3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_vec3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_saturate_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_snoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_vec_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_srandom3_vec_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley2_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley2_value_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley3_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&_lpfx_worley3_value_q32_fn as *const _);
    }
}
