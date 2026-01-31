//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

use lp_glsl_builtins::builtins::lpfx::color::space::hue2rgb_f32::__lpfx_hue2rgb_f32;
use lp_glsl_builtins::builtins::lpfx::color::space::hue2rgb_q32::__lpfx_hue2rgb_q32;
use lp_glsl_builtins::builtins::lpfx::color::space::{
    hsv2rgb_f32::__lpfx_hsv2rgb_f32, hsv2rgb_f32::__lpfx_hsv2rgb_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    hsv2rgb_q32::__lpfx_hsv2rgb_q32, hsv2rgb_q32::__lpfx_hsv2rgb_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    rgb2hsv_f32::__lpfx_rgb2hsv_f32, rgb2hsv_f32::__lpfx_rgb2hsv_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::color::space::{
    rgb2hsv_q32::__lpfx_rgb2hsv_q32, rgb2hsv_q32::__lpfx_rgb2hsv_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm2_f32::__lpfx_fbm2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm2_q32::__lpfx_fbm2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_f32::__lpfx_fbm3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_q32::__lpfx_fbm3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_tile_f32::__lpfx_fbm3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::fbm::fbm3_tile_q32::__lpfx_fbm3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise1_f32::__lpfx_gnoise1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise1_q32::__lpfx_gnoise1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise2_f32::__lpfx_gnoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise2_q32::__lpfx_gnoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_f32::__lpfx_gnoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_q32::__lpfx_gnoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_tile_f32::__lpfx_gnoise3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::gnoise::gnoise3_tile_q32::__lpfx_gnoise3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise2_f32::__lpfx_psrdnoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise2_q32::__lpfx_psrdnoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise3_f32::__lpfx_psrdnoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::psrdnoise::psrdnoise3_q32::__lpfx_psrdnoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random1_f32::__lpfx_random1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random1_q32::__lpfx_random1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random2_f32::__lpfx_random2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random2_q32::__lpfx_random2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random3_f32::__lpfx_random3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::random::random3_q32::__lpfx_random3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise1_f32::__lpfx_snoise1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise1_q32::__lpfx_snoise1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise2_f32::__lpfx_snoise2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise2_q32::__lpfx_snoise2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise3_f32::__lpfx_snoise3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::snoise::snoise3_q32::__lpfx_snoise3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom1_f32::__lpfx_srandom1_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom1_q32::__lpfx_srandom1_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom2_f32::__lpfx_srandom2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom2_q32::__lpfx_srandom2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_f32::__lpfx_srandom3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_q32::__lpfx_srandom3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_tile_f32::__lpfx_srandom3_tile_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_tile_q32::__lpfx_srandom3_tile_q32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_vec_f32::__lpfx_srandom3_vec_f32;
use lp_glsl_builtins::builtins::lpfx::generative::srandom::srandom3_vec_q32::__lpfx_srandom3_vec_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_f32::__lpfx_worley2_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_q32::__lpfx_worley2_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_value_f32::__lpfx_worley2_value_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley2_value_q32::__lpfx_worley2_value_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_f32::__lpfx_worley3_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_q32::__lpfx_worley3_q32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_value_f32::__lpfx_worley3_value_f32;
use lp_glsl_builtins::builtins::lpfx::generative::worley::worley3_value_q32::__lpfx_worley3_value_q32;
use lp_glsl_builtins::builtins::lpfx::math::{
    saturate_f32::__lpfx_saturate_f32, saturate_f32::__lpfx_saturate_vec3_f32,
    saturate_f32::__lpfx_saturate_vec4_f32,
};
use lp_glsl_builtins::builtins::lpfx::math::{
    saturate_q32::__lpfx_saturate_q32, saturate_q32::__lpfx_saturate_vec3_q32,
    saturate_q32::__lpfx_saturate_vec4_q32,
};
use lp_glsl_builtins::builtins::lpfx::{
    hash::__lpfx_hash_1, hash::__lpfx_hash_2, hash::__lpfx_hash_3,
};
use lp_glsl_builtins::builtins::q32::__lp_q32_acos;
use lp_glsl_builtins::builtins::q32::__lp_q32_acosh;
use lp_glsl_builtins::builtins::q32::__lp_q32_add;
use lp_glsl_builtins::builtins::q32::__lp_q32_asin;
use lp_glsl_builtins::builtins::q32::__lp_q32_asinh;
use lp_glsl_builtins::builtins::q32::__lp_q32_atan;
use lp_glsl_builtins::builtins::q32::__lp_q32_atan2;
use lp_glsl_builtins::builtins::q32::__lp_q32_atanh;
use lp_glsl_builtins::builtins::q32::__lp_q32_cos;
use lp_glsl_builtins::builtins::q32::__lp_q32_cosh;
use lp_glsl_builtins::builtins::q32::__lp_q32_div;
use lp_glsl_builtins::builtins::q32::__lp_q32_exp;
use lp_glsl_builtins::builtins::q32::__lp_q32_exp2;
use lp_glsl_builtins::builtins::q32::__lp_q32_fma;
use lp_glsl_builtins::builtins::q32::__lp_q32_inversesqrt;
use lp_glsl_builtins::builtins::q32::__lp_q32_ldexp;
use lp_glsl_builtins::builtins::q32::__lp_q32_log;
use lp_glsl_builtins::builtins::q32::__lp_q32_log2;
use lp_glsl_builtins::builtins::q32::__lp_q32_mod;
use lp_glsl_builtins::builtins::q32::__lp_q32_mul;
use lp_glsl_builtins::builtins::q32::__lp_q32_pow;
use lp_glsl_builtins::builtins::q32::__lp_q32_round;
use lp_glsl_builtins::builtins::q32::__lp_q32_roundeven;
use lp_glsl_builtins::builtins::q32::__lp_q32_sin;
use lp_glsl_builtins::builtins::q32::__lp_q32_sinh;
use lp_glsl_builtins::builtins::q32::__lp_q32_sqrt;
use lp_glsl_builtins::builtins::q32::__lp_q32_sub;
use lp_glsl_builtins::builtins::q32::__lp_q32_tan;
use lp_glsl_builtins::builtins::q32::__lp_q32_tanh;

/// Reference all builtin functions to prevent dead code elimination.
///
/// This function ensures all builtin functions are included in the executable
/// by creating function pointers and reading them with volatile operations.
pub fn ensure_builtins_referenced() {
    unsafe {
        let _q32_acos_fn: extern "C" fn(i32) -> i32 = __lp_q32_acos;
        let _q32_acosh_fn: extern "C" fn(i32) -> i32 = __lp_q32_acosh;
        let _q32_add_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_add;
        let _q32_asin_fn: extern "C" fn(i32) -> i32 = __lp_q32_asin;
        let _q32_asinh_fn: extern "C" fn(i32) -> i32 = __lp_q32_asinh;
        let _q32_atan_fn: extern "C" fn(i32) -> i32 = __lp_q32_atan;
        let _q32_atan2_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_atan2;
        let _q32_atanh_fn: extern "C" fn(i32) -> i32 = __lp_q32_atanh;
        let _q32_cos_fn: extern "C" fn(i32) -> i32 = __lp_q32_cos;
        let _q32_cosh_fn: extern "C" fn(i32) -> i32 = __lp_q32_cosh;
        let _q32_div_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_div;
        let _q32_exp_fn: extern "C" fn(i32) -> i32 = __lp_q32_exp;
        let _q32_exp2_fn: extern "C" fn(i32) -> i32 = __lp_q32_exp2;
        let _q32_fma_fn: extern "C" fn(i32, i32, i32) -> i32 = __lp_q32_fma;
        let _q32_inversesqrt_fn: extern "C" fn(i32) -> i32 = __lp_q32_inversesqrt;
        let _q32_ldexp_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_ldexp;
        let _q32_log_fn: extern "C" fn(i32) -> i32 = __lp_q32_log;
        let _q32_log2_fn: extern "C" fn(i32) -> i32 = __lp_q32_log2;
        let _q32_mod_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_mod;
        let _q32_mul_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_mul;
        let _q32_pow_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_pow;
        let _q32_round_fn: extern "C" fn(i32) -> i32 = __lp_q32_round;
        let _q32_roundeven_fn: extern "C" fn(i32) -> i32 = __lp_q32_roundeven;
        let _q32_sin_fn: extern "C" fn(i32) -> i32 = __lp_q32_sin;
        let _q32_sinh_fn: extern "C" fn(i32) -> i32 = __lp_q32_sinh;
        let _q32_sqrt_fn: extern "C" fn(i32) -> i32 = __lp_q32_sqrt;
        let _q32_sub_fn: extern "C" fn(i32, i32) -> i32 = __lp_q32_sub;
        let _q32_tan_fn: extern "C" fn(i32) -> i32 = __lp_q32_tan;
        let _q32_tanh_fn: extern "C" fn(i32) -> i32 = __lp_q32_tanh;
        let __lpfx_fbm2_f32_fn: extern "C" fn(f32, f32, i32, u32) -> f32 = __lpfx_fbm2_f32;
        let __lpfx_fbm2_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_fbm2_q32;
        let __lpfx_fbm3_f32_fn: extern "C" fn(f32, f32, f32, i32, u32) -> f32 = __lpfx_fbm3_f32;
        let __lpfx_fbm3_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 = __lpfx_fbm3_q32;
        let __lpfx_fbm3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, i32, u32) -> f32 =
            __lpfx_fbm3_tile_f32;
        let __lpfx_fbm3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, u32) -> i32 =
            __lpfx_fbm3_tile_q32;
        let __lpfx_gnoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lpfx_gnoise1_f32;
        let __lpfx_gnoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lpfx_gnoise1_q32;
        let __lpfx_gnoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lpfx_gnoise2_f32;
        let __lpfx_gnoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lpfx_gnoise2_q32;
        let __lpfx_gnoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lpfx_gnoise3_f32;
        let __lpfx_gnoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_gnoise3_q32;
        let __lpfx_gnoise3_tile_f32_fn: extern "C" fn(f32, f32, f32, f32, u32) -> f32 =
            __lpfx_gnoise3_tile_f32;
        let __lpfx_gnoise3_tile_q32_fn: extern "C" fn(i32, i32, i32, i32, u32) -> i32 =
            __lpfx_gnoise3_tile_q32;
        let __lpfx_hash_1_fn: extern "C" fn(u32, u32) -> u32 = __lpfx_hash_1;
        let __lpfx_hash_2_fn: extern "C" fn(u32, u32, u32) -> u32 = __lpfx_hash_2;
        let __lpfx_hash_3_fn: extern "C" fn(u32, u32, u32, u32) -> u32 = __lpfx_hash_3;
        let __lpfx_hsv2rgb_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lpfx_hsv2rgb_f32;
        let __lpfx_hsv2rgb_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lpfx_hsv2rgb_q32;
        let __lpfx_hsv2rgb_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lpfx_hsv2rgb_vec4_f32;
        let __lpfx_hsv2rgb_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lpfx_hsv2rgb_vec4_q32;
        let __lpfx_hue2rgb_f32_fn: extern "C" fn(*mut f32, f32) -> () = __lpfx_hue2rgb_f32;
        let __lpfx_hue2rgb_q32_fn: extern "C" fn(*mut i32, i32) -> () = __lpfx_hue2rgb_q32;
        let __lpfx_psrdnoise2_f32_fn: extern "C" fn(f32, f32, f32, f32, f32, *mut f32, u32) -> f32 =
            __lpfx_psrdnoise2_f32;
        let __lpfx_psrdnoise2_q32_fn: extern "C" fn(i32, i32, i32, i32, i32, *mut i32, u32) -> i32 =
            __lpfx_psrdnoise2_q32;
        let __lpfx_psrdnoise3_f32_fn: extern "C" fn(
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            *mut f32,
            u32,
        ) -> f32 = __lpfx_psrdnoise3_f32;
        let __lpfx_psrdnoise3_q32_fn: extern "C" fn(
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            *mut i32,
            u32,
        ) -> i32 = __lpfx_psrdnoise3_q32;
        let __lpfx_random1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lpfx_random1_f32;
        let __lpfx_random1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lpfx_random1_q32;
        let __lpfx_random2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lpfx_random2_f32;
        let __lpfx_random2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lpfx_random2_q32;
        let __lpfx_random3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lpfx_random3_f32;
        let __lpfx_random3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_random3_q32;
        let __lpfx_rgb2hsv_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lpfx_rgb2hsv_f32;
        let __lpfx_rgb2hsv_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lpfx_rgb2hsv_q32;
        let __lpfx_rgb2hsv_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lpfx_rgb2hsv_vec4_f32;
        let __lpfx_rgb2hsv_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lpfx_rgb2hsv_vec4_q32;
        let __lpfx_saturate_f32_fn: extern "C" fn(f32) -> f32 = __lpfx_saturate_f32;
        let __lpfx_saturate_q32_fn: extern "C" fn(i32) -> i32 = __lpfx_saturate_q32;
        let __lpfx_saturate_vec3_f32_fn: extern "C" fn(*mut f32, f32, f32, f32) -> () =
            __lpfx_saturate_vec3_f32;
        let __lpfx_saturate_vec3_q32_fn: extern "C" fn(*mut i32, i32, i32, i32) -> () =
            __lpfx_saturate_vec3_q32;
        let __lpfx_saturate_vec4_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32) -> () =
            __lpfx_saturate_vec4_f32;
        let __lpfx_saturate_vec4_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32) -> () =
            __lpfx_saturate_vec4_q32;
        let __lpfx_snoise1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lpfx_snoise1_f32;
        let __lpfx_snoise1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lpfx_snoise1_q32;
        let __lpfx_snoise2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lpfx_snoise2_f32;
        let __lpfx_snoise2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lpfx_snoise2_q32;
        let __lpfx_snoise3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lpfx_snoise3_f32;
        let __lpfx_snoise3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_snoise3_q32;
        let __lpfx_srandom1_f32_fn: extern "C" fn(f32, u32) -> f32 = __lpfx_srandom1_f32;
        let __lpfx_srandom1_q32_fn: extern "C" fn(i32, u32) -> i32 = __lpfx_srandom1_q32;
        let __lpfx_srandom2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lpfx_srandom2_f32;
        let __lpfx_srandom2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lpfx_srandom2_q32;
        let __lpfx_srandom3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lpfx_srandom3_f32;
        let __lpfx_srandom3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_srandom3_q32;
        let __lpfx_srandom3_tile_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, f32, u32) -> () =
            __lpfx_srandom3_tile_f32;
        let __lpfx_srandom3_tile_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, i32, u32) -> () =
            __lpfx_srandom3_tile_q32;
        let __lpfx_srandom3_vec_f32_fn: extern "C" fn(*mut f32, f32, f32, f32, u32) -> () =
            __lpfx_srandom3_vec_f32;
        let __lpfx_srandom3_vec_q32_fn: extern "C" fn(*mut i32, i32, i32, i32, u32) -> () =
            __lpfx_srandom3_vec_q32;
        let __lpfx_worley2_f32_fn: extern "C" fn(f32, f32, u32) -> f32 = __lpfx_worley2_f32;
        let __lpfx_worley2_q32_fn: extern "C" fn(i32, i32, u32) -> i32 = __lpfx_worley2_q32;
        let __lpfx_worley2_value_f32_fn: extern "C" fn(f32, f32, u32) -> f32 =
            __lpfx_worley2_value_f32;
        let __lpfx_worley2_value_q32_fn: extern "C" fn(i32, i32, u32) -> i32 =
            __lpfx_worley2_value_q32;
        let __lpfx_worley3_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 = __lpfx_worley3_f32;
        let __lpfx_worley3_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 = __lpfx_worley3_q32;
        let __lpfx_worley3_value_f32_fn: extern "C" fn(f32, f32, f32, u32) -> f32 =
            __lpfx_worley3_value_f32;
        let __lpfx_worley3_value_q32_fn: extern "C" fn(i32, i32, i32, u32) -> i32 =
            __lpfx_worley3_value_q32;

        // Force these to be included by using them in a way that can't be optimized away
        // We'll use volatile reads to prevent optimization
        let _ = core::ptr::read_volatile(&_q32_acos_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_acosh_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_add_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_asin_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_asinh_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_atan_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_atan2_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_atanh_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_cos_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_cosh_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_div_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_exp_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_exp2_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_fma_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_inversesqrt_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_ldexp_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_log_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_log2_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_mod_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_mul_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_pow_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_round_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_roundeven_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_sin_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_sinh_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_sqrt_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_sub_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_tan_fn as *const _);
        let _ = core::ptr::read_volatile(&_q32_tanh_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_fbm3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_gnoise3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hash_1_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hash_2_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hash_3_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hsv2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hsv2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hsv2rgb_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hsv2rgb_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hue2rgb_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_hue2rgb_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_psrdnoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_psrdnoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_psrdnoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_psrdnoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_random3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_rgb2hsv_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_rgb2hsv_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_rgb2hsv_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_rgb2hsv_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_vec3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_vec3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_vec4_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_saturate_vec4_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_snoise3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom1_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom1_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_tile_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_tile_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_vec_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_srandom3_vec_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley2_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley2_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley2_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley2_value_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley3_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley3_q32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley3_value_f32_fn as *const _);
        let _ = core::ptr::read_volatile(&__lpfx_worley3_value_q32_fn as *const _);
    }
}
