//! JIT builtin symbols and LPIR import resolution (shared with `lp-glsl-cranelift` registry).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use lp_glsl_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id,
    lpir_q32_builtin_id,
};
use lpir::FloatMode;
use lpir::module::{ImportDecl, IrModule};

use crate::error::CompileError;

pub(crate) fn cranelift_sig_for_builtin(
    builtin: BuiltinId,
    pointer_type: types::Type,
    call_conv: CallConv,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    match builtin {
        BuiltinId::LpLpfxPsrdnoise2F32 | BuiltinId::LpLpfxPsrdnoise2Q32 => {
            // Out parameter function: (5 i32 params, pointer_type) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfxPsrdnoise3F32 | BuiltinId::LpLpfxPsrdnoise3Q32 => {
            // Out parameter function: (7 i32 params, pointer_type) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfxSrandom3TileF32 | BuiltinId::LpLpfxSrandom3TileQ32 => {
            // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32, i32) -> ()
            sig.params.insert(0, AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            // Functions with result pointer return void
        }
        BuiltinId::LpLpfxHsv2rgbVec4F32
        | BuiltinId::LpLpfxHsv2rgbVec4Q32
        | BuiltinId::LpLpfxRgb2hsvVec4F32
        | BuiltinId::LpLpfxRgb2hsvVec4Q32
        | BuiltinId::LpLpfxSaturateVec4F32
        | BuiltinId::LpLpfxSaturateVec4Q32
        | BuiltinId::LpLpfxSrandom3VecF32
        | BuiltinId::LpLpfxSrandom3VecQ32 => {
            // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32) -> ()
            sig.params.insert(0, AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            // Functions with result pointer return void
        }
        BuiltinId::LpLpfxHsv2rgbF32
        | BuiltinId::LpLpfxHsv2rgbQ32
        | BuiltinId::LpLpfxRgb2hsvF32
        | BuiltinId::LpLpfxRgb2hsvQ32
        | BuiltinId::LpLpfxSaturateVec3F32
        | BuiltinId::LpLpfxSaturateVec3Q32 => {
            // Result pointer as normal parameter: (pointer_type, i32, i32, i32) -> ()
            sig.params.insert(0, AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            // Functions with result pointer return void
        }
        BuiltinId::LpLpfxHue2rgbF32 | BuiltinId::LpLpfxHue2rgbQ32 => {
            // Result pointer as normal parameter: (pointer_type, i32) -> ()
            sig.params.insert(0, AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            // Functions with result pointer return void
        }
        BuiltinId::LpLpfxFbm3TileF32 | BuiltinId::LpLpfxFbm3TileQ32 => {
            // (i32, i32, i32, i32, i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfxFbm3F32
        | BuiltinId::LpLpfxFbm3Q32
        | BuiltinId::LpLpfxGnoise3TileF32
        | BuiltinId::LpLpfxGnoise3TileQ32 => {
            // (i32, i32, i32, i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfxFbm2F32
        | BuiltinId::LpLpfxFbm2Q32
        | BuiltinId::LpLpfxGnoise3F32
        | BuiltinId::LpLpfxGnoise3Q32
        | BuiltinId::LpLpfxHash3
        | BuiltinId::LpLpfxRandom3F32
        | BuiltinId::LpLpfxRandom3Q32
        | BuiltinId::LpLpfxSnoise3F32
        | BuiltinId::LpLpfxSnoise3Q32
        | BuiltinId::LpLpfxSrandom3F32
        | BuiltinId::LpLpfxSrandom3Q32
        | BuiltinId::LpLpfxWorley3F32
        | BuiltinId::LpLpfxWorley3Q32
        | BuiltinId::LpLpfxWorley3ValueF32
        | BuiltinId::LpLpfxWorley3ValueQ32 => {
            // (i32, i32, i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslFmaQ32
        | BuiltinId::LpLpfxGnoise2F32
        | BuiltinId::LpLpfxGnoise2Q32
        | BuiltinId::LpLpfxHash2
        | BuiltinId::LpLpfxRandom2F32
        | BuiltinId::LpLpfxRandom2Q32
        | BuiltinId::LpLpfxSnoise2F32
        | BuiltinId::LpLpfxSnoise2Q32
        | BuiltinId::LpLpfxSrandom2F32
        | BuiltinId::LpLpfxSrandom2Q32
        | BuiltinId::LpLpfxWorley2F32
        | BuiltinId::LpLpfxWorley2Q32
        | BuiltinId::LpLpfxWorley2ValueF32
        | BuiltinId::LpLpfxWorley2ValueQ32 => {
            // (i32, i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslAtan2Q32
        | BuiltinId::LpGlslLdexpQ32
        | BuiltinId::LpGlslModQ32
        | BuiltinId::LpGlslPowQ32
        | BuiltinId::LpLpirFaddQ32
        | BuiltinId::LpLpirFdivQ32
        | BuiltinId::LpLpirFmulQ32
        | BuiltinId::LpLpirFsubQ32
        | BuiltinId::LpLpfxGnoise1F32
        | BuiltinId::LpLpfxGnoise1Q32
        | BuiltinId::LpLpfxHash1
        | BuiltinId::LpLpfxRandom1F32
        | BuiltinId::LpLpfxRandom1Q32
        | BuiltinId::LpLpfxSnoise1F32
        | BuiltinId::LpLpfxSnoise1Q32
        | BuiltinId::LpLpfxSrandom1F32
        | BuiltinId::LpLpfxSrandom1Q32 => {
            // (i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslAcosQ32
        | BuiltinId::LpGlslAcoshQ32
        | BuiltinId::LpGlslAsinQ32
        | BuiltinId::LpGlslAsinhQ32
        | BuiltinId::LpGlslAtanQ32
        | BuiltinId::LpGlslAtanhQ32
        | BuiltinId::LpGlslCosQ32
        | BuiltinId::LpGlslCoshQ32
        | BuiltinId::LpGlslExp2Q32
        | BuiltinId::LpGlslExpQ32
        | BuiltinId::LpGlslInversesqrtQ32
        | BuiltinId::LpGlslLog2Q32
        | BuiltinId::LpGlslLogQ32
        | BuiltinId::LpGlslRoundQ32
        | BuiltinId::LpGlslSinQ32
        | BuiltinId::LpGlslSinhQ32
        | BuiltinId::LpGlslTanQ32
        | BuiltinId::LpGlslTanhQ32
        | BuiltinId::LpLpirFnearestQ32
        | BuiltinId::LpLpirFsqrtQ32
        | BuiltinId::LpLpfxSaturateF32
        | BuiltinId::LpLpfxSaturateQ32 => {
            // (i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
    }
    sig
}
pub(crate) fn get_function_pointer(builtin: BuiltinId) -> *const u8 {
    use lp_glsl_builtins::builtins::{
        glsl::{
            acos_q32, acosh_q32, asin_q32, asinh_q32, atan_q32, atan2_q32, atanh_q32, cos_q32,
            cosh_q32, exp_q32, exp2_q32, fma_q32, inversesqrt_q32, ldexp_q32, log_q32, log2_q32,
            mod_q32, pow_q32, round_q32, sin_q32, sinh_q32, tan_q32, tanh_q32,
        },
        lpfx::color,
        lpfx::generative,
        lpfx::hash,
        lpfx::math,
        lpir::{fadd_q32, fdiv_q32, fmul_q32, fnearest_q32, fsqrt_q32, fsub_q32},
    };
    match builtin {
        BuiltinId::LpGlslAcosQ32 => acos_q32::__lp_glsl_acos_q32 as *const u8,
        BuiltinId::LpGlslAcoshQ32 => acosh_q32::__lp_glsl_acosh_q32 as *const u8,
        BuiltinId::LpGlslAsinQ32 => asin_q32::__lp_glsl_asin_q32 as *const u8,
        BuiltinId::LpGlslAsinhQ32 => asinh_q32::__lp_glsl_asinh_q32 as *const u8,
        BuiltinId::LpGlslAtan2Q32 => atan2_q32::__lp_glsl_atan2_q32 as *const u8,
        BuiltinId::LpGlslAtanQ32 => atan_q32::__lp_glsl_atan_q32 as *const u8,
        BuiltinId::LpGlslAtanhQ32 => atanh_q32::__lp_glsl_atanh_q32 as *const u8,
        BuiltinId::LpGlslCosQ32 => cos_q32::__lp_glsl_cos_q32 as *const u8,
        BuiltinId::LpGlslCoshQ32 => cosh_q32::__lp_glsl_cosh_q32 as *const u8,
        BuiltinId::LpGlslExp2Q32 => exp2_q32::__lp_glsl_exp2_q32 as *const u8,
        BuiltinId::LpGlslExpQ32 => exp_q32::__lp_glsl_exp_q32 as *const u8,
        BuiltinId::LpGlslFmaQ32 => fma_q32::__lp_glsl_fma_q32 as *const u8,
        BuiltinId::LpGlslInversesqrtQ32 => inversesqrt_q32::__lp_glsl_inversesqrt_q32 as *const u8,
        BuiltinId::LpGlslLdexpQ32 => ldexp_q32::__lp_glsl_ldexp_q32 as *const u8,
        BuiltinId::LpGlslLog2Q32 => log2_q32::__lp_glsl_log2_q32 as *const u8,
        BuiltinId::LpGlslLogQ32 => log_q32::__lp_glsl_log_q32 as *const u8,
        BuiltinId::LpGlslModQ32 => mod_q32::__lp_glsl_mod_q32 as *const u8,
        BuiltinId::LpGlslPowQ32 => pow_q32::__lp_glsl_pow_q32 as *const u8,
        BuiltinId::LpGlslRoundQ32 => round_q32::__lp_glsl_round_q32 as *const u8,
        BuiltinId::LpGlslSinQ32 => sin_q32::__lp_glsl_sin_q32 as *const u8,
        BuiltinId::LpGlslSinhQ32 => sinh_q32::__lp_glsl_sinh_q32 as *const u8,
        BuiltinId::LpGlslTanQ32 => tan_q32::__lp_glsl_tan_q32 as *const u8,
        BuiltinId::LpGlslTanhQ32 => tanh_q32::__lp_glsl_tanh_q32 as *const u8,
        BuiltinId::LpLpirFaddQ32 => fadd_q32::__lp_lpir_fadd_q32 as *const u8,
        BuiltinId::LpLpirFdivQ32 => fdiv_q32::__lp_lpir_fdiv_q32 as *const u8,
        BuiltinId::LpLpirFmulQ32 => fmul_q32::__lp_lpir_fmul_q32 as *const u8,
        BuiltinId::LpLpirFnearestQ32 => fnearest_q32::__lp_lpir_fnearest_q32 as *const u8,
        BuiltinId::LpLpirFsqrtQ32 => fsqrt_q32::__lp_lpir_fsqrt_q32 as *const u8,
        BuiltinId::LpLpirFsubQ32 => fsub_q32::__lp_lpir_fsub_q32 as *const u8,
        BuiltinId::LpLpfxFbm2F32 => generative::fbm::fbm2_f32::__lp_lpfx_fbm2_f32 as *const u8,
        BuiltinId::LpLpfxFbm2Q32 => generative::fbm::fbm2_q32::__lp_lpfx_fbm2_q32 as *const u8,
        BuiltinId::LpLpfxFbm3F32 => generative::fbm::fbm3_f32::__lp_lpfx_fbm3_f32 as *const u8,
        BuiltinId::LpLpfxFbm3Q32 => generative::fbm::fbm3_q32::__lp_lpfx_fbm3_q32 as *const u8,
        BuiltinId::LpLpfxFbm3TileF32 => {
            generative::fbm::fbm3_tile_f32::__lp_lpfx_fbm3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfxFbm3TileQ32 => {
            generative::fbm::fbm3_tile_q32::__lp_lpfx_fbm3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfxGnoise1F32 => {
            generative::gnoise::gnoise1_f32::__lp_lpfx_gnoise1_f32 as *const u8
        }
        BuiltinId::LpLpfxGnoise1Q32 => {
            generative::gnoise::gnoise1_q32::__lp_lpfx_gnoise1_q32 as *const u8
        }
        BuiltinId::LpLpfxGnoise2F32 => {
            generative::gnoise::gnoise2_f32::__lp_lpfx_gnoise2_f32 as *const u8
        }
        BuiltinId::LpLpfxGnoise2Q32 => {
            generative::gnoise::gnoise2_q32::__lp_lpfx_gnoise2_q32 as *const u8
        }
        BuiltinId::LpLpfxGnoise3F32 => {
            generative::gnoise::gnoise3_f32::__lp_lpfx_gnoise3_f32 as *const u8
        }
        BuiltinId::LpLpfxGnoise3Q32 => {
            generative::gnoise::gnoise3_q32::__lp_lpfx_gnoise3_q32 as *const u8
        }
        BuiltinId::LpLpfxGnoise3TileF32 => {
            generative::gnoise::gnoise3_tile_f32::__lp_lpfx_gnoise3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfxGnoise3TileQ32 => {
            generative::gnoise::gnoise3_tile_q32::__lp_lpfx_gnoise3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfxHash1 => hash::__lp_lpfx_hash_1 as *const u8,
        BuiltinId::LpLpfxHash2 => hash::__lp_lpfx_hash_2 as *const u8,
        BuiltinId::LpLpfxHash3 => hash::__lp_lpfx_hash_3 as *const u8,
        BuiltinId::LpLpfxHsv2rgbF32 => {
            color::space::hsv2rgb_f32::__lp_lpfx_hsv2rgb_f32 as *const u8
        }
        BuiltinId::LpLpfxHsv2rgbQ32 => {
            color::space::hsv2rgb_q32::__lp_lpfx_hsv2rgb_q32 as *const u8
        }
        BuiltinId::LpLpfxHsv2rgbVec4F32 => {
            color::space::hsv2rgb_f32::__lp_lpfx_hsv2rgb_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfxHsv2rgbVec4Q32 => {
            color::space::hsv2rgb_q32::__lp_lpfx_hsv2rgb_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfxHue2rgbF32 => {
            color::space::hue2rgb_f32::__lp_lpfx_hue2rgb_f32 as *const u8
        }
        BuiltinId::LpLpfxHue2rgbQ32 => {
            color::space::hue2rgb_q32::__lp_lpfx_hue2rgb_q32 as *const u8
        }
        BuiltinId::LpLpfxPsrdnoise2F32 => {
            generative::psrdnoise::psrdnoise2_f32::__lp_lpfx_psrdnoise2_f32 as *const u8
        }
        BuiltinId::LpLpfxPsrdnoise2Q32 => {
            generative::psrdnoise::psrdnoise2_q32::__lp_lpfx_psrdnoise2_q32 as *const u8
        }
        BuiltinId::LpLpfxPsrdnoise3F32 => {
            generative::psrdnoise::psrdnoise3_f32::__lp_lpfx_psrdnoise3_f32 as *const u8
        }
        BuiltinId::LpLpfxPsrdnoise3Q32 => {
            generative::psrdnoise::psrdnoise3_q32::__lp_lpfx_psrdnoise3_q32 as *const u8
        }
        BuiltinId::LpLpfxRandom1F32 => {
            generative::random::random1_f32::__lp_lpfx_random1_f32 as *const u8
        }
        BuiltinId::LpLpfxRandom1Q32 => {
            generative::random::random1_q32::__lp_lpfx_random1_q32 as *const u8
        }
        BuiltinId::LpLpfxRandom2F32 => {
            generative::random::random2_f32::__lp_lpfx_random2_f32 as *const u8
        }
        BuiltinId::LpLpfxRandom2Q32 => {
            generative::random::random2_q32::__lp_lpfx_random2_q32 as *const u8
        }
        BuiltinId::LpLpfxRandom3F32 => {
            generative::random::random3_f32::__lp_lpfx_random3_f32 as *const u8
        }
        BuiltinId::LpLpfxRandom3Q32 => {
            generative::random::random3_q32::__lp_lpfx_random3_q32 as *const u8
        }
        BuiltinId::LpLpfxRgb2hsvF32 => {
            color::space::rgb2hsv_f32::__lp_lpfx_rgb2hsv_f32 as *const u8
        }
        BuiltinId::LpLpfxRgb2hsvQ32 => {
            color::space::rgb2hsv_q32::__lp_lpfx_rgb2hsv_q32 as *const u8
        }
        BuiltinId::LpLpfxRgb2hsvVec4F32 => {
            color::space::rgb2hsv_f32::__lp_lpfx_rgb2hsv_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfxRgb2hsvVec4Q32 => {
            color::space::rgb2hsv_q32::__lp_lpfx_rgb2hsv_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfxSaturateF32 => math::saturate_f32::__lp_lpfx_saturate_f32 as *const u8,
        BuiltinId::LpLpfxSaturateQ32 => math::saturate_q32::__lp_lpfx_saturate_q32 as *const u8,
        BuiltinId::LpLpfxSaturateVec3F32 => {
            math::saturate_f32::__lp_lpfx_saturate_vec3_f32 as *const u8
        }
        BuiltinId::LpLpfxSaturateVec3Q32 => {
            math::saturate_q32::__lp_lpfx_saturate_vec3_q32 as *const u8
        }
        BuiltinId::LpLpfxSaturateVec4F32 => {
            math::saturate_f32::__lp_lpfx_saturate_vec4_f32 as *const u8
        }
        BuiltinId::LpLpfxSaturateVec4Q32 => {
            math::saturate_q32::__lp_lpfx_saturate_vec4_q32 as *const u8
        }
        BuiltinId::LpLpfxSnoise1F32 => {
            generative::snoise::snoise1_f32::__lp_lpfx_snoise1_f32 as *const u8
        }
        BuiltinId::LpLpfxSnoise1Q32 => {
            generative::snoise::snoise1_q32::__lp_lpfx_snoise1_q32 as *const u8
        }
        BuiltinId::LpLpfxSnoise2F32 => {
            generative::snoise::snoise2_f32::__lp_lpfx_snoise2_f32 as *const u8
        }
        BuiltinId::LpLpfxSnoise2Q32 => {
            generative::snoise::snoise2_q32::__lp_lpfx_snoise2_q32 as *const u8
        }
        BuiltinId::LpLpfxSnoise3F32 => {
            generative::snoise::snoise3_f32::__lp_lpfx_snoise3_f32 as *const u8
        }
        BuiltinId::LpLpfxSnoise3Q32 => {
            generative::snoise::snoise3_q32::__lp_lpfx_snoise3_q32 as *const u8
        }
        BuiltinId::LpLpfxSrandom1F32 => {
            generative::srandom::srandom1_f32::__lp_lpfx_srandom1_f32 as *const u8
        }
        BuiltinId::LpLpfxSrandom1Q32 => {
            generative::srandom::srandom1_q32::__lp_lpfx_srandom1_q32 as *const u8
        }
        BuiltinId::LpLpfxSrandom2F32 => {
            generative::srandom::srandom2_f32::__lp_lpfx_srandom2_f32 as *const u8
        }
        BuiltinId::LpLpfxSrandom2Q32 => {
            generative::srandom::srandom2_q32::__lp_lpfx_srandom2_q32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3F32 => {
            generative::srandom::srandom3_f32::__lp_lpfx_srandom3_f32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3Q32 => {
            generative::srandom::srandom3_q32::__lp_lpfx_srandom3_q32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3TileF32 => {
            generative::srandom::srandom3_tile_f32::__lp_lpfx_srandom3_tile_f32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3TileQ32 => {
            generative::srandom::srandom3_tile_q32::__lp_lpfx_srandom3_tile_q32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3VecF32 => {
            generative::srandom::srandom3_vec_f32::__lp_lpfx_srandom3_vec_f32 as *const u8
        }
        BuiltinId::LpLpfxSrandom3VecQ32 => {
            generative::srandom::srandom3_vec_q32::__lp_lpfx_srandom3_vec_q32 as *const u8
        }
        BuiltinId::LpLpfxWorley2F32 => {
            generative::worley::worley2_f32::__lp_lpfx_worley2_f32 as *const u8
        }
        BuiltinId::LpLpfxWorley2Q32 => {
            generative::worley::worley2_q32::__lp_lpfx_worley2_q32 as *const u8
        }
        BuiltinId::LpLpfxWorley2ValueF32 => {
            generative::worley::worley2_value_f32::__lp_lpfx_worley2_value_f32 as *const u8
        }
        BuiltinId::LpLpfxWorley2ValueQ32 => {
            generative::worley::worley2_value_q32::__lp_lpfx_worley2_value_q32 as *const u8
        }
        BuiltinId::LpLpfxWorley3F32 => {
            generative::worley::worley3_f32::__lp_lpfx_worley3_f32 as *const u8
        }
        BuiltinId::LpLpfxWorley3Q32 => {
            generative::worley::worley3_q32::__lp_lpfx_worley3_q32 as *const u8
        }
        BuiltinId::LpLpfxWorley3ValueF32 => {
            generative::worley::worley3_value_f32::__lp_lpfx_worley3_value_f32 as *const u8
        }
        BuiltinId::LpLpfxWorley3ValueQ32 => {
            generative::worley::worley3_value_q32::__lp_lpfx_worley3_value_q32 as *const u8
        }
    }
}

pub(crate) fn resolve_import(
    decl: &ImportDecl,
    mode: FloatMode,
) -> Result<BuiltinId, CompileError> {
    match (decl.module_name.as_str(), mode) {
        ("glsl", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported glsl import `{}` (arity {ac})",
                    decl.func_name
                ))
            })
        }
        ("lpir", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported lpir import `{}` (arity {ac})",
                    decl.func_name
                ))
            })
        }
        ("lpfx", FloatMode::Q32) => {
            let base = lpfx_strip_suffix(&decl.func_name)?;
            let kinds = lpfx_glsl_kinds_from_decl(decl)?;
            glsl_lpfx_q32_builtin_id(base, &kinds).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported lpfx import `{}` with kinds {:?}",
                    decl.func_name, kinds
                ))
            })
        }
        ("glsl" | "lpir" | "lpfx", FloatMode::F32) => Err(CompileError::unsupported(format!(
            "import `{}::{}` requires FloatMode::Q32",
            decl.module_name, decl.func_name
        ))),
        (m, _) => Err(CompileError::unsupported(format!(
            "unsupported import module `{m}`"
        ))),
    }
}

pub(crate) struct LpirBuiltinFuncIds {
    pub fadd: FuncId,
    pub fsub: FuncId,
    pub fmul: FuncId,
    pub fdiv: FuncId,
    pub fsqrt: FuncId,
    pub fnearest: FuncId,
}

pub(crate) fn declare_module_imports(
    module: &mut JITModule,
    ir: &IrModule,
    pointer_type: types::Type,
) -> Result<Vec<FuncId>, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let mut out = Vec::with_capacity(ir.imports.len());
    for decl in &ir.imports {
        let bid = resolve_import(decl, FloatMode::Q32)?;
        let sig = cranelift_sig_for_builtin(bid, pointer_type, call_conv);
        let id = module
            .declare_function(bid.name(), Linkage::Import, &sig)
            .map_err(|e| CompileError::cranelift(format!("declare import {}: {e}", bid.name())))?;
        out.push(id);
    }
    Ok(out)
}

pub(crate) fn declare_lpir_opcode_builtins(
    module: &mut JITModule,
    pointer_type: types::Type,
) -> Result<LpirBuiltinFuncIds, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let mut declare = |bid: BuiltinId| -> Result<FuncId, CompileError> {
        let sig = cranelift_sig_for_builtin(bid, pointer_type, call_conv);
        module
            .declare_function(bid.name(), Linkage::Import, &sig)
            .map_err(|e| {
                CompileError::cranelift(format!("declare LPIR opcode builtin {}: {e}", bid.name()))
            })
    };
    Ok(LpirBuiltinFuncIds {
        fadd: declare(BuiltinId::LpLpirFaddQ32)?,
        fsub: declare(BuiltinId::LpLpirFsubQ32)?,
        fmul: declare(BuiltinId::LpLpirFmulQ32)?,
        fdiv: declare(BuiltinId::LpLpirFdivQ32)?,
        fsqrt: declare(BuiltinId::LpLpirFsqrtQ32)?,
        fnearest: declare(BuiltinId::LpLpirFnearestQ32)?,
    })
}

pub(crate) fn symbol_lookup_fn() -> Box<dyn Fn(&str) -> Option<*const u8> + Send> {
    Box::new(|name: &str| {
        for builtin in BuiltinId::all() {
            if builtin.name() == name {
                return Some(get_function_pointer(*builtin));
            }
        }
        None
    })
}

fn ir_params_to_glsl_kinds(params: &[lpir::types::IrType]) -> Vec<GlslParamKind> {
    params
        .iter()
        .map(|t| match t {
            lpir::types::IrType::F32 => GlslParamKind::Float,
            lpir::types::IrType::I32 => GlslParamKind::UInt,
        })
        .collect()
}

fn lpfx_glsl_kinds_from_decl(decl: &ImportDecl) -> Result<Vec<GlslParamKind>, CompileError> {
    if let Some(ref enc) = decl.lpfx_glsl_params {
        parse_lpfx_glsl_params_csv(enc).map_err(CompileError::unsupported)
    } else {
        Ok(ir_params_to_glsl_kinds(&decl.param_types))
    }
}

fn parse_lpfx_glsl_params_csv(enc: &str) -> Result<Vec<GlslParamKind>, String> {
    if enc.is_empty() {
        return Ok(Vec::new());
    }
    enc.split(',')
        .map(|t| match t.trim() {
            "Float" => Ok(GlslParamKind::Float),
            "Int" => Ok(GlslParamKind::Int),
            "UInt" => Ok(GlslParamKind::UInt),
            "Vec2" => Ok(GlslParamKind::Vec2),
            "Vec3" => Ok(GlslParamKind::Vec3),
            "Vec4" => Ok(GlslParamKind::Vec4),
            "IVec2" => Ok(GlslParamKind::IVec2),
            "IVec3" => Ok(GlslParamKind::IVec3),
            "IVec4" => Ok(GlslParamKind::IVec4),
            "UVec2" => Ok(GlslParamKind::UVec2),
            "UVec3" => Ok(GlslParamKind::UVec3),
            "UVec4" => Ok(GlslParamKind::UVec4),
            "BVec2" => Ok(GlslParamKind::BVec2),
            "BVec3" => Ok(GlslParamKind::BVec3),
            "BVec4" => Ok(GlslParamKind::BVec4),
            other => Err(format!("unknown LPFX glsl param tag `{other}`")),
        })
        .collect()
}

fn lpfx_strip_suffix(func_name: &str) -> Result<&str, CompileError> {
    let (base, tail) = func_name.rsplit_once('_').ok_or_else(|| {
        CompileError::unsupported(format!("malformed lpfx import name `{func_name}`"))
    })?;
    tail.parse::<u32>().map_err(|_| {
        CompileError::unsupported(format!("malformed lpfx import name `{func_name}`"))
    })?;
    Ok(base)
}
