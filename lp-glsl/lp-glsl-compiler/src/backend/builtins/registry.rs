//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! Builtin function registry implementation.
//!
//! Provides enum-based registry for builtin functions with support for both
//! JIT (function pointer) and emulator (ELF symbol) linking.

use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{Linkage, Module};

#[cfg(not(feature = "std"))]
use alloc::format;

/// Enum identifying builtin functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinId {
    LpQ32Acos,
    LpQ32Acosh,
    LpQ32Add,
    LpQ32Asin,
    LpQ32Asinh,
    LpQ32Atan,
    LpQ32Atan2,
    LpQ32Atanh,
    LpQ32Cos,
    LpQ32Cosh,
    LpQ32Div,
    LpQ32Exp,
    LpQ32Exp2,
    LpQ32Fma,
    LpQ32Inversesqrt,
    LpQ32Ldexp,
    LpQ32Log,
    LpQ32Log2,
    LpQ32Mod,
    LpQ32Mul,
    LpQ32Pow,
    LpQ32Round,
    LpQ32Roundeven,
    LpQ32Sin,
    LpQ32Sinh,
    LpQ32Sqrt,
    LpQ32Sub,
    LpQ32Tan,
    LpQ32Tanh,
    LpfxFbm2F32,
    LpfxFbm2Q32,
    LpfxFbm3F32,
    LpfxFbm3Q32,
    LpfxFbm3TileF32,
    LpfxFbm3TileQ32,
    LpfxGnoise1F32,
    LpfxGnoise1Q32,
    LpfxGnoise2F32,
    LpfxGnoise2Q32,
    LpfxGnoise3F32,
    LpfxGnoise3Q32,
    LpfxGnoise3TileF32,
    LpfxGnoise3TileQ32,
    LpfxHash1,
    LpfxHash2,
    LpfxHash3,
    LpfxHsv2rgbF32,
    LpfxHsv2rgbQ32,
    LpfxHsv2rgbVec4F32,
    LpfxHsv2rgbVec4Q32,
    LpfxHue2rgbF32,
    LpfxHue2rgbQ32,
    LpfxPsrdnoise2F32,
    LpfxPsrdnoise2Q32,
    LpfxPsrdnoise3F32,
    LpfxPsrdnoise3Q32,
    LpfxRandom1F32,
    LpfxRandom1Q32,
    LpfxRandom2F32,
    LpfxRandom2Q32,
    LpfxRandom3F32,
    LpfxRandom3Q32,
    LpfxRgb2hsvF32,
    LpfxRgb2hsvQ32,
    LpfxRgb2hsvVec4F32,
    LpfxRgb2hsvVec4Q32,
    LpfxSaturateF32,
    LpfxSaturateQ32,
    LpfxSaturateVec3F32,
    LpfxSaturateVec3Q32,
    LpfxSaturateVec4F32,
    LpfxSaturateVec4Q32,
    LpfxSnoise1F32,
    LpfxSnoise1Q32,
    LpfxSnoise2F32,
    LpfxSnoise2Q32,
    LpfxSnoise3F32,
    LpfxSnoise3Q32,
    LpfxSrandom1F32,
    LpfxSrandom1Q32,
    LpfxSrandom2F32,
    LpfxSrandom2Q32,
    LpfxSrandom3F32,
    LpfxSrandom3Q32,
    LpfxSrandom3TileF32,
    LpfxSrandom3TileQ32,
    LpfxSrandom3VecF32,
    LpfxSrandom3VecQ32,
    LpfxWorley2F32,
    LpfxWorley2Q32,
    LpfxWorley2ValueF32,
    LpfxWorley2ValueQ32,
    LpfxWorley3F32,
    LpfxWorley3Q32,
    LpfxWorley3ValueF32,
    LpfxWorley3ValueQ32,
}

impl BuiltinId {
    /// Get the symbol name for this builtin function.
    pub fn name(&self) -> &'static str {
        match self {
            BuiltinId::LpQ32Acos => "__lp_q32_acos",
            BuiltinId::LpQ32Acosh => "__lp_q32_acosh",
            BuiltinId::LpQ32Add => "__lp_q32_add",
            BuiltinId::LpQ32Asin => "__lp_q32_asin",
            BuiltinId::LpQ32Asinh => "__lp_q32_asinh",
            BuiltinId::LpQ32Atan => "__lp_q32_atan",
            BuiltinId::LpQ32Atan2 => "__lp_q32_atan2",
            BuiltinId::LpQ32Atanh => "__lp_q32_atanh",
            BuiltinId::LpQ32Cos => "__lp_q32_cos",
            BuiltinId::LpQ32Cosh => "__lp_q32_cosh",
            BuiltinId::LpQ32Div => "__lp_q32_div",
            BuiltinId::LpQ32Exp => "__lp_q32_exp",
            BuiltinId::LpQ32Exp2 => "__lp_q32_exp2",
            BuiltinId::LpQ32Fma => "__lp_q32_fma",
            BuiltinId::LpQ32Inversesqrt => "__lp_q32_inversesqrt",
            BuiltinId::LpQ32Ldexp => "__lp_q32_ldexp",
            BuiltinId::LpQ32Log => "__lp_q32_log",
            BuiltinId::LpQ32Log2 => "__lp_q32_log2",
            BuiltinId::LpQ32Mod => "__lp_q32_mod",
            BuiltinId::LpQ32Mul => "__lp_q32_mul",
            BuiltinId::LpQ32Pow => "__lp_q32_pow",
            BuiltinId::LpQ32Round => "__lp_q32_round",
            BuiltinId::LpQ32Roundeven => "__lp_q32_roundeven",
            BuiltinId::LpQ32Sin => "__lp_q32_sin",
            BuiltinId::LpQ32Sinh => "__lp_q32_sinh",
            BuiltinId::LpQ32Sqrt => "__lp_q32_sqrt",
            BuiltinId::LpQ32Sub => "__lp_q32_sub",
            BuiltinId::LpQ32Tan => "__lp_q32_tan",
            BuiltinId::LpQ32Tanh => "__lp_q32_tanh",
            BuiltinId::LpfxFbm2F32 => "__lpfx_fbm2_f32",
            BuiltinId::LpfxFbm2Q32 => "__lpfx_fbm2_q32",
            BuiltinId::LpfxFbm3F32 => "__lpfx_fbm3_f32",
            BuiltinId::LpfxFbm3Q32 => "__lpfx_fbm3_q32",
            BuiltinId::LpfxFbm3TileF32 => "__lpfx_fbm3_tile_f32",
            BuiltinId::LpfxFbm3TileQ32 => "__lpfx_fbm3_tile_q32",
            BuiltinId::LpfxGnoise1F32 => "__lpfx_gnoise1_f32",
            BuiltinId::LpfxGnoise1Q32 => "__lpfx_gnoise1_q32",
            BuiltinId::LpfxGnoise2F32 => "__lpfx_gnoise2_f32",
            BuiltinId::LpfxGnoise2Q32 => "__lpfx_gnoise2_q32",
            BuiltinId::LpfxGnoise3F32 => "__lpfx_gnoise3_f32",
            BuiltinId::LpfxGnoise3Q32 => "__lpfx_gnoise3_q32",
            BuiltinId::LpfxGnoise3TileF32 => "__lpfx_gnoise3_tile_f32",
            BuiltinId::LpfxGnoise3TileQ32 => "__lpfx_gnoise3_tile_q32",
            BuiltinId::LpfxHash1 => "__lpfx_hash_1",
            BuiltinId::LpfxHash2 => "__lpfx_hash_2",
            BuiltinId::LpfxHash3 => "__lpfx_hash_3",
            BuiltinId::LpfxHsv2rgbF32 => "__lpfx_hsv2rgb_f32",
            BuiltinId::LpfxHsv2rgbQ32 => "__lpfx_hsv2rgb_q32",
            BuiltinId::LpfxHsv2rgbVec4F32 => "__lpfx_hsv2rgb_vec4_f32",
            BuiltinId::LpfxHsv2rgbVec4Q32 => "__lpfx_hsv2rgb_vec4_q32",
            BuiltinId::LpfxHue2rgbF32 => "__lpfx_hue2rgb_f32",
            BuiltinId::LpfxHue2rgbQ32 => "__lpfx_hue2rgb_q32",
            BuiltinId::LpfxPsrdnoise2F32 => "__lpfx_psrdnoise2_f32",
            BuiltinId::LpfxPsrdnoise2Q32 => "__lpfx_psrdnoise2_q32",
            BuiltinId::LpfxPsrdnoise3F32 => "__lpfx_psrdnoise3_f32",
            BuiltinId::LpfxPsrdnoise3Q32 => "__lpfx_psrdnoise3_q32",
            BuiltinId::LpfxRandom1F32 => "__lpfx_random1_f32",
            BuiltinId::LpfxRandom1Q32 => "__lpfx_random1_q32",
            BuiltinId::LpfxRandom2F32 => "__lpfx_random2_f32",
            BuiltinId::LpfxRandom2Q32 => "__lpfx_random2_q32",
            BuiltinId::LpfxRandom3F32 => "__lpfx_random3_f32",
            BuiltinId::LpfxRandom3Q32 => "__lpfx_random3_q32",
            BuiltinId::LpfxRgb2hsvF32 => "__lpfx_rgb2hsv_f32",
            BuiltinId::LpfxRgb2hsvQ32 => "__lpfx_rgb2hsv_q32",
            BuiltinId::LpfxRgb2hsvVec4F32 => "__lpfx_rgb2hsv_vec4_f32",
            BuiltinId::LpfxRgb2hsvVec4Q32 => "__lpfx_rgb2hsv_vec4_q32",
            BuiltinId::LpfxSaturateF32 => "__lpfx_saturate_f32",
            BuiltinId::LpfxSaturateQ32 => "__lpfx_saturate_q32",
            BuiltinId::LpfxSaturateVec3F32 => "__lpfx_saturate_vec3_f32",
            BuiltinId::LpfxSaturateVec3Q32 => "__lpfx_saturate_vec3_q32",
            BuiltinId::LpfxSaturateVec4F32 => "__lpfx_saturate_vec4_f32",
            BuiltinId::LpfxSaturateVec4Q32 => "__lpfx_saturate_vec4_q32",
            BuiltinId::LpfxSnoise1F32 => "__lpfx_snoise1_f32",
            BuiltinId::LpfxSnoise1Q32 => "__lpfx_snoise1_q32",
            BuiltinId::LpfxSnoise2F32 => "__lpfx_snoise2_f32",
            BuiltinId::LpfxSnoise2Q32 => "__lpfx_snoise2_q32",
            BuiltinId::LpfxSnoise3F32 => "__lpfx_snoise3_f32",
            BuiltinId::LpfxSnoise3Q32 => "__lpfx_snoise3_q32",
            BuiltinId::LpfxSrandom1F32 => "__lpfx_srandom1_f32",
            BuiltinId::LpfxSrandom1Q32 => "__lpfx_srandom1_q32",
            BuiltinId::LpfxSrandom2F32 => "__lpfx_srandom2_f32",
            BuiltinId::LpfxSrandom2Q32 => "__lpfx_srandom2_q32",
            BuiltinId::LpfxSrandom3F32 => "__lpfx_srandom3_f32",
            BuiltinId::LpfxSrandom3Q32 => "__lpfx_srandom3_q32",
            BuiltinId::LpfxSrandom3TileF32 => "__lpfx_srandom3_tile_f32",
            BuiltinId::LpfxSrandom3TileQ32 => "__lpfx_srandom3_tile_q32",
            BuiltinId::LpfxSrandom3VecF32 => "__lpfx_srandom3_vec_f32",
            BuiltinId::LpfxSrandom3VecQ32 => "__lpfx_srandom3_vec_q32",
            BuiltinId::LpfxWorley2F32 => "__lpfx_worley2_f32",
            BuiltinId::LpfxWorley2Q32 => "__lpfx_worley2_q32",
            BuiltinId::LpfxWorley2ValueF32 => "__lpfx_worley2_value_f32",
            BuiltinId::LpfxWorley2ValueQ32 => "__lpfx_worley2_value_q32",
            BuiltinId::LpfxWorley3F32 => "__lpfx_worley3_f32",
            BuiltinId::LpfxWorley3Q32 => "__lpfx_worley3_q32",
            BuiltinId::LpfxWorley3ValueF32 => "__lpfx_worley3_value_f32",
            BuiltinId::LpfxWorley3ValueQ32 => "__lpfx_worley3_value_q32",
        }
    }

    /// Get the BuiltinId from its symbol name.
    ///
    /// Returns `None` if the name is not a known builtin function.
    pub fn builtin_id_from_name(name: &str) -> Option<BuiltinId> {
        match name {
            "__lp_q32_acos" => Some(BuiltinId::LpQ32Acos),
            "__lp_q32_acosh" => Some(BuiltinId::LpQ32Acosh),
            "__lp_q32_add" => Some(BuiltinId::LpQ32Add),
            "__lp_q32_asin" => Some(BuiltinId::LpQ32Asin),
            "__lp_q32_asinh" => Some(BuiltinId::LpQ32Asinh),
            "__lp_q32_atan" => Some(BuiltinId::LpQ32Atan),
            "__lp_q32_atan2" => Some(BuiltinId::LpQ32Atan2),
            "__lp_q32_atanh" => Some(BuiltinId::LpQ32Atanh),
            "__lp_q32_cos" => Some(BuiltinId::LpQ32Cos),
            "__lp_q32_cosh" => Some(BuiltinId::LpQ32Cosh),
            "__lp_q32_div" => Some(BuiltinId::LpQ32Div),
            "__lp_q32_exp" => Some(BuiltinId::LpQ32Exp),
            "__lp_q32_exp2" => Some(BuiltinId::LpQ32Exp2),
            "__lp_q32_fma" => Some(BuiltinId::LpQ32Fma),
            "__lp_q32_inversesqrt" => Some(BuiltinId::LpQ32Inversesqrt),
            "__lp_q32_ldexp" => Some(BuiltinId::LpQ32Ldexp),
            "__lp_q32_log" => Some(BuiltinId::LpQ32Log),
            "__lp_q32_log2" => Some(BuiltinId::LpQ32Log2),
            "__lp_q32_mod" => Some(BuiltinId::LpQ32Mod),
            "__lp_q32_mul" => Some(BuiltinId::LpQ32Mul),
            "__lp_q32_pow" => Some(BuiltinId::LpQ32Pow),
            "__lp_q32_round" => Some(BuiltinId::LpQ32Round),
            "__lp_q32_roundeven" => Some(BuiltinId::LpQ32Roundeven),
            "__lp_q32_sin" => Some(BuiltinId::LpQ32Sin),
            "__lp_q32_sinh" => Some(BuiltinId::LpQ32Sinh),
            "__lp_q32_sqrt" => Some(BuiltinId::LpQ32Sqrt),
            "__lp_q32_sub" => Some(BuiltinId::LpQ32Sub),
            "__lp_q32_tan" => Some(BuiltinId::LpQ32Tan),
            "__lp_q32_tanh" => Some(BuiltinId::LpQ32Tanh),
            "__lpfx_fbm2_f32" => Some(BuiltinId::LpfxFbm2F32),
            "__lpfx_fbm2_q32" => Some(BuiltinId::LpfxFbm2Q32),
            "__lpfx_fbm3_f32" => Some(BuiltinId::LpfxFbm3F32),
            "__lpfx_fbm3_q32" => Some(BuiltinId::LpfxFbm3Q32),
            "__lpfx_fbm3_tile_f32" => Some(BuiltinId::LpfxFbm3TileF32),
            "__lpfx_fbm3_tile_q32" => Some(BuiltinId::LpfxFbm3TileQ32),
            "__lpfx_gnoise1_f32" => Some(BuiltinId::LpfxGnoise1F32),
            "__lpfx_gnoise1_q32" => Some(BuiltinId::LpfxGnoise1Q32),
            "__lpfx_gnoise2_f32" => Some(BuiltinId::LpfxGnoise2F32),
            "__lpfx_gnoise2_q32" => Some(BuiltinId::LpfxGnoise2Q32),
            "__lpfx_gnoise3_f32" => Some(BuiltinId::LpfxGnoise3F32),
            "__lpfx_gnoise3_q32" => Some(BuiltinId::LpfxGnoise3Q32),
            "__lpfx_gnoise3_tile_f32" => Some(BuiltinId::LpfxGnoise3TileF32),
            "__lpfx_gnoise3_tile_q32" => Some(BuiltinId::LpfxGnoise3TileQ32),
            "__lpfx_hash_1" => Some(BuiltinId::LpfxHash1),
            "__lpfx_hash_2" => Some(BuiltinId::LpfxHash2),
            "__lpfx_hash_3" => Some(BuiltinId::LpfxHash3),
            "__lpfx_hsv2rgb_f32" => Some(BuiltinId::LpfxHsv2rgbF32),
            "__lpfx_hsv2rgb_q32" => Some(BuiltinId::LpfxHsv2rgbQ32),
            "__lpfx_hsv2rgb_vec4_f32" => Some(BuiltinId::LpfxHsv2rgbVec4F32),
            "__lpfx_hsv2rgb_vec4_q32" => Some(BuiltinId::LpfxHsv2rgbVec4Q32),
            "__lpfx_hue2rgb_f32" => Some(BuiltinId::LpfxHue2rgbF32),
            "__lpfx_hue2rgb_q32" => Some(BuiltinId::LpfxHue2rgbQ32),
            "__lpfx_psrdnoise2_f32" => Some(BuiltinId::LpfxPsrdnoise2F32),
            "__lpfx_psrdnoise2_q32" => Some(BuiltinId::LpfxPsrdnoise2Q32),
            "__lpfx_psrdnoise3_f32" => Some(BuiltinId::LpfxPsrdnoise3F32),
            "__lpfx_psrdnoise3_q32" => Some(BuiltinId::LpfxPsrdnoise3Q32),
            "__lpfx_random1_f32" => Some(BuiltinId::LpfxRandom1F32),
            "__lpfx_random1_q32" => Some(BuiltinId::LpfxRandom1Q32),
            "__lpfx_random2_f32" => Some(BuiltinId::LpfxRandom2F32),
            "__lpfx_random2_q32" => Some(BuiltinId::LpfxRandom2Q32),
            "__lpfx_random3_f32" => Some(BuiltinId::LpfxRandom3F32),
            "__lpfx_random3_q32" => Some(BuiltinId::LpfxRandom3Q32),
            "__lpfx_rgb2hsv_f32" => Some(BuiltinId::LpfxRgb2hsvF32),
            "__lpfx_rgb2hsv_q32" => Some(BuiltinId::LpfxRgb2hsvQ32),
            "__lpfx_rgb2hsv_vec4_f32" => Some(BuiltinId::LpfxRgb2hsvVec4F32),
            "__lpfx_rgb2hsv_vec4_q32" => Some(BuiltinId::LpfxRgb2hsvVec4Q32),
            "__lpfx_saturate_f32" => Some(BuiltinId::LpfxSaturateF32),
            "__lpfx_saturate_q32" => Some(BuiltinId::LpfxSaturateQ32),
            "__lpfx_saturate_vec3_f32" => Some(BuiltinId::LpfxSaturateVec3F32),
            "__lpfx_saturate_vec3_q32" => Some(BuiltinId::LpfxSaturateVec3Q32),
            "__lpfx_saturate_vec4_f32" => Some(BuiltinId::LpfxSaturateVec4F32),
            "__lpfx_saturate_vec4_q32" => Some(BuiltinId::LpfxSaturateVec4Q32),
            "__lpfx_snoise1_f32" => Some(BuiltinId::LpfxSnoise1F32),
            "__lpfx_snoise1_q32" => Some(BuiltinId::LpfxSnoise1Q32),
            "__lpfx_snoise2_f32" => Some(BuiltinId::LpfxSnoise2F32),
            "__lpfx_snoise2_q32" => Some(BuiltinId::LpfxSnoise2Q32),
            "__lpfx_snoise3_f32" => Some(BuiltinId::LpfxSnoise3F32),
            "__lpfx_snoise3_q32" => Some(BuiltinId::LpfxSnoise3Q32),
            "__lpfx_srandom1_f32" => Some(BuiltinId::LpfxSrandom1F32),
            "__lpfx_srandom1_q32" => Some(BuiltinId::LpfxSrandom1Q32),
            "__lpfx_srandom2_f32" => Some(BuiltinId::LpfxSrandom2F32),
            "__lpfx_srandom2_q32" => Some(BuiltinId::LpfxSrandom2Q32),
            "__lpfx_srandom3_f32" => Some(BuiltinId::LpfxSrandom3F32),
            "__lpfx_srandom3_q32" => Some(BuiltinId::LpfxSrandom3Q32),
            "__lpfx_srandom3_tile_f32" => Some(BuiltinId::LpfxSrandom3TileF32),
            "__lpfx_srandom3_tile_q32" => Some(BuiltinId::LpfxSrandom3TileQ32),
            "__lpfx_srandom3_vec_f32" => Some(BuiltinId::LpfxSrandom3VecF32),
            "__lpfx_srandom3_vec_q32" => Some(BuiltinId::LpfxSrandom3VecQ32),
            "__lpfx_worley2_f32" => Some(BuiltinId::LpfxWorley2F32),
            "__lpfx_worley2_q32" => Some(BuiltinId::LpfxWorley2Q32),
            "__lpfx_worley2_value_f32" => Some(BuiltinId::LpfxWorley2ValueF32),
            "__lpfx_worley2_value_q32" => Some(BuiltinId::LpfxWorley2ValueQ32),
            "__lpfx_worley3_f32" => Some(BuiltinId::LpfxWorley3F32),
            "__lpfx_worley3_q32" => Some(BuiltinId::LpfxWorley3Q32),
            "__lpfx_worley3_value_f32" => Some(BuiltinId::LpfxWorley3ValueF32),
            "__lpfx_worley3_value_q32" => Some(BuiltinId::LpfxWorley3ValueQ32),
            _ => None,
        }
    }

    /// Get the Cranelift signature for this builtin function.
    ///
    /// `pointer_type` is the native pointer type for the target architecture.
    /// For RISC-V 32-bit, this should be `types::I32`.
    /// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.
    pub fn signature(&self, pointer_type: types::Type) -> Signature {
        let mut sig = Signature::new(CallConv::SystemV);
        match self {
            BuiltinId::LpfxPsrdnoise2F32 | BuiltinId::LpfxPsrdnoise2Q32 => {
                // Out parameter function: (5 i32 params, pointer_type) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(pointer_type));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpfxPsrdnoise3F32 | BuiltinId::LpfxPsrdnoise3Q32 => {
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
            BuiltinId::LpfxSrandom3TileF32 | BuiltinId::LpfxSrandom3TileQ32 => {
                // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32, i32) -> ()
                sig.params.insert(0, AbiParam::new(pointer_type));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                // Functions with result pointer return void
            }
            BuiltinId::LpfxHsv2rgbVec4F32
            | BuiltinId::LpfxHsv2rgbVec4Q32
            | BuiltinId::LpfxRgb2hsvVec4F32
            | BuiltinId::LpfxRgb2hsvVec4Q32
            | BuiltinId::LpfxSaturateVec4F32
            | BuiltinId::LpfxSaturateVec4Q32
            | BuiltinId::LpfxSrandom3VecF32
            | BuiltinId::LpfxSrandom3VecQ32 => {
                // Result pointer as normal parameter: (pointer_type, i32, i32, i32, i32) -> ()
                sig.params.insert(0, AbiParam::new(pointer_type));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                // Functions with result pointer return void
            }
            BuiltinId::LpfxHsv2rgbF32
            | BuiltinId::LpfxHsv2rgbQ32
            | BuiltinId::LpfxRgb2hsvF32
            | BuiltinId::LpfxRgb2hsvQ32
            | BuiltinId::LpfxSaturateVec3F32
            | BuiltinId::LpfxSaturateVec3Q32 => {
                // Result pointer as normal parameter: (pointer_type, i32, i32, i32) -> ()
                sig.params.insert(0, AbiParam::new(pointer_type));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                // Functions with result pointer return void
            }
            BuiltinId::LpfxHue2rgbF32 | BuiltinId::LpfxHue2rgbQ32 => {
                // Result pointer as normal parameter: (pointer_type, i32) -> ()
                sig.params.insert(0, AbiParam::new(pointer_type));
                sig.params.push(AbiParam::new(types::I32));
                // Functions with result pointer return void
            }
            BuiltinId::LpfxFbm3TileF32 | BuiltinId::LpfxFbm3TileQ32 => {
                // (i32, i32, i32, i32, i32, i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpfxFbm3F32
            | BuiltinId::LpfxFbm3Q32
            | BuiltinId::LpfxGnoise3TileF32
            | BuiltinId::LpfxGnoise3TileQ32 => {
                // (i32, i32, i32, i32, i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpfxFbm2F32
            | BuiltinId::LpfxFbm2Q32
            | BuiltinId::LpfxGnoise3F32
            | BuiltinId::LpfxGnoise3Q32
            | BuiltinId::LpfxHash3
            | BuiltinId::LpfxRandom3F32
            | BuiltinId::LpfxRandom3Q32
            | BuiltinId::LpfxSnoise3F32
            | BuiltinId::LpfxSnoise3Q32
            | BuiltinId::LpfxSrandom3F32
            | BuiltinId::LpfxSrandom3Q32
            | BuiltinId::LpfxWorley3F32
            | BuiltinId::LpfxWorley3Q32
            | BuiltinId::LpfxWorley3ValueF32
            | BuiltinId::LpfxWorley3ValueQ32 => {
                // (i32, i32, i32, i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpQ32Fma
            | BuiltinId::LpfxGnoise2F32
            | BuiltinId::LpfxGnoise2Q32
            | BuiltinId::LpfxHash2
            | BuiltinId::LpfxRandom2F32
            | BuiltinId::LpfxRandom2Q32
            | BuiltinId::LpfxSnoise2F32
            | BuiltinId::LpfxSnoise2Q32
            | BuiltinId::LpfxSrandom2F32
            | BuiltinId::LpfxSrandom2Q32
            | BuiltinId::LpfxWorley2F32
            | BuiltinId::LpfxWorley2Q32
            | BuiltinId::LpfxWorley2ValueF32
            | BuiltinId::LpfxWorley2ValueQ32 => {
                // (i32, i32, i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpQ32Add
            | BuiltinId::LpQ32Atan2
            | BuiltinId::LpQ32Div
            | BuiltinId::LpQ32Ldexp
            | BuiltinId::LpQ32Mod
            | BuiltinId::LpQ32Mul
            | BuiltinId::LpQ32Pow
            | BuiltinId::LpQ32Sub
            | BuiltinId::LpfxGnoise1F32
            | BuiltinId::LpfxGnoise1Q32
            | BuiltinId::LpfxHash1
            | BuiltinId::LpfxRandom1F32
            | BuiltinId::LpfxRandom1Q32
            | BuiltinId::LpfxSnoise1F32
            | BuiltinId::LpfxSnoise1Q32
            | BuiltinId::LpfxSrandom1F32
            | BuiltinId::LpfxSrandom1Q32 => {
                // (i32, i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
            BuiltinId::LpQ32Acos
            | BuiltinId::LpQ32Acosh
            | BuiltinId::LpQ32Asin
            | BuiltinId::LpQ32Asinh
            | BuiltinId::LpQ32Atan
            | BuiltinId::LpQ32Atanh
            | BuiltinId::LpQ32Cos
            | BuiltinId::LpQ32Cosh
            | BuiltinId::LpQ32Exp
            | BuiltinId::LpQ32Exp2
            | BuiltinId::LpQ32Inversesqrt
            | BuiltinId::LpQ32Log
            | BuiltinId::LpQ32Log2
            | BuiltinId::LpQ32Round
            | BuiltinId::LpQ32Roundeven
            | BuiltinId::LpQ32Sin
            | BuiltinId::LpQ32Sinh
            | BuiltinId::LpQ32Sqrt
            | BuiltinId::LpQ32Tan
            | BuiltinId::LpQ32Tanh
            | BuiltinId::LpfxSaturateF32
            | BuiltinId::LpfxSaturateQ32 => {
                // (i32) -> i32
                sig.params.push(AbiParam::new(types::I32));
                sig.returns.push(AbiParam::new(types::I32));
            }
        }
        sig
    }

    /// Get all builtin IDs.
    pub fn all() -> &'static [BuiltinId] {
        &[
            BuiltinId::LpQ32Acos,
            BuiltinId::LpQ32Acosh,
            BuiltinId::LpQ32Add,
            BuiltinId::LpQ32Asin,
            BuiltinId::LpQ32Asinh,
            BuiltinId::LpQ32Atan,
            BuiltinId::LpQ32Atan2,
            BuiltinId::LpQ32Atanh,
            BuiltinId::LpQ32Cos,
            BuiltinId::LpQ32Cosh,
            BuiltinId::LpQ32Div,
            BuiltinId::LpQ32Exp,
            BuiltinId::LpQ32Exp2,
            BuiltinId::LpQ32Fma,
            BuiltinId::LpQ32Inversesqrt,
            BuiltinId::LpQ32Ldexp,
            BuiltinId::LpQ32Log,
            BuiltinId::LpQ32Log2,
            BuiltinId::LpQ32Mod,
            BuiltinId::LpQ32Mul,
            BuiltinId::LpQ32Pow,
            BuiltinId::LpQ32Round,
            BuiltinId::LpQ32Roundeven,
            BuiltinId::LpQ32Sin,
            BuiltinId::LpQ32Sinh,
            BuiltinId::LpQ32Sqrt,
            BuiltinId::LpQ32Sub,
            BuiltinId::LpQ32Tan,
            BuiltinId::LpQ32Tanh,
            BuiltinId::LpfxFbm2F32,
            BuiltinId::LpfxFbm2Q32,
            BuiltinId::LpfxFbm3F32,
            BuiltinId::LpfxFbm3Q32,
            BuiltinId::LpfxFbm3TileF32,
            BuiltinId::LpfxFbm3TileQ32,
            BuiltinId::LpfxGnoise1F32,
            BuiltinId::LpfxGnoise1Q32,
            BuiltinId::LpfxGnoise2F32,
            BuiltinId::LpfxGnoise2Q32,
            BuiltinId::LpfxGnoise3F32,
            BuiltinId::LpfxGnoise3Q32,
            BuiltinId::LpfxGnoise3TileF32,
            BuiltinId::LpfxGnoise3TileQ32,
            BuiltinId::LpfxHash1,
            BuiltinId::LpfxHash2,
            BuiltinId::LpfxHash3,
            BuiltinId::LpfxHsv2rgbF32,
            BuiltinId::LpfxHsv2rgbQ32,
            BuiltinId::LpfxHsv2rgbVec4F32,
            BuiltinId::LpfxHsv2rgbVec4Q32,
            BuiltinId::LpfxHue2rgbF32,
            BuiltinId::LpfxHue2rgbQ32,
            BuiltinId::LpfxPsrdnoise2F32,
            BuiltinId::LpfxPsrdnoise2Q32,
            BuiltinId::LpfxPsrdnoise3F32,
            BuiltinId::LpfxPsrdnoise3Q32,
            BuiltinId::LpfxRandom1F32,
            BuiltinId::LpfxRandom1Q32,
            BuiltinId::LpfxRandom2F32,
            BuiltinId::LpfxRandom2Q32,
            BuiltinId::LpfxRandom3F32,
            BuiltinId::LpfxRandom3Q32,
            BuiltinId::LpfxRgb2hsvF32,
            BuiltinId::LpfxRgb2hsvQ32,
            BuiltinId::LpfxRgb2hsvVec4F32,
            BuiltinId::LpfxRgb2hsvVec4Q32,
            BuiltinId::LpfxSaturateF32,
            BuiltinId::LpfxSaturateQ32,
            BuiltinId::LpfxSaturateVec3F32,
            BuiltinId::LpfxSaturateVec3Q32,
            BuiltinId::LpfxSaturateVec4F32,
            BuiltinId::LpfxSaturateVec4Q32,
            BuiltinId::LpfxSnoise1F32,
            BuiltinId::LpfxSnoise1Q32,
            BuiltinId::LpfxSnoise2F32,
            BuiltinId::LpfxSnoise2Q32,
            BuiltinId::LpfxSnoise3F32,
            BuiltinId::LpfxSnoise3Q32,
            BuiltinId::LpfxSrandom1F32,
            BuiltinId::LpfxSrandom1Q32,
            BuiltinId::LpfxSrandom2F32,
            BuiltinId::LpfxSrandom2Q32,
            BuiltinId::LpfxSrandom3F32,
            BuiltinId::LpfxSrandom3Q32,
            BuiltinId::LpfxSrandom3TileF32,
            BuiltinId::LpfxSrandom3TileQ32,
            BuiltinId::LpfxSrandom3VecF32,
            BuiltinId::LpfxSrandom3VecQ32,
            BuiltinId::LpfxWorley2F32,
            BuiltinId::LpfxWorley2Q32,
            BuiltinId::LpfxWorley2ValueF32,
            BuiltinId::LpfxWorley2ValueQ32,
            BuiltinId::LpfxWorley3F32,
            BuiltinId::LpfxWorley3Q32,
            BuiltinId::LpfxWorley3ValueF32,
            BuiltinId::LpfxWorley3ValueQ32,
        ]
    }
}

/// Get function pointer for a builtin (JIT mode only).
///
/// Returns the function pointer that can be registered with JITModule.
pub fn get_function_pointer(builtin: BuiltinId) -> *const u8 {
    use lp_glsl_builtins::builtins::{lpfx::color, lpfx::generative, lpfx::hash, lpfx::math, q32};
    match builtin {
        BuiltinId::LpQ32Acos => q32::__lp_q32_acos as *const u8,
        BuiltinId::LpQ32Acosh => q32::__lp_q32_acosh as *const u8,
        BuiltinId::LpQ32Add => q32::__lp_q32_add as *const u8,
        BuiltinId::LpQ32Asin => q32::__lp_q32_asin as *const u8,
        BuiltinId::LpQ32Asinh => q32::__lp_q32_asinh as *const u8,
        BuiltinId::LpQ32Atan => q32::__lp_q32_atan as *const u8,
        BuiltinId::LpQ32Atan2 => q32::__lp_q32_atan2 as *const u8,
        BuiltinId::LpQ32Atanh => q32::__lp_q32_atanh as *const u8,
        BuiltinId::LpQ32Cos => q32::__lp_q32_cos as *const u8,
        BuiltinId::LpQ32Cosh => q32::__lp_q32_cosh as *const u8,
        BuiltinId::LpQ32Div => q32::__lp_q32_div as *const u8,
        BuiltinId::LpQ32Exp => q32::__lp_q32_exp as *const u8,
        BuiltinId::LpQ32Exp2 => q32::__lp_q32_exp2 as *const u8,
        BuiltinId::LpQ32Fma => q32::__lp_q32_fma as *const u8,
        BuiltinId::LpQ32Inversesqrt => q32::__lp_q32_inversesqrt as *const u8,
        BuiltinId::LpQ32Ldexp => q32::__lp_q32_ldexp as *const u8,
        BuiltinId::LpQ32Log => q32::__lp_q32_log as *const u8,
        BuiltinId::LpQ32Log2 => q32::__lp_q32_log2 as *const u8,
        BuiltinId::LpQ32Mod => q32::__lp_q32_mod as *const u8,
        BuiltinId::LpQ32Mul => q32::__lp_q32_mul as *const u8,
        BuiltinId::LpQ32Pow => q32::__lp_q32_pow as *const u8,
        BuiltinId::LpQ32Round => q32::__lp_q32_round as *const u8,
        BuiltinId::LpQ32Roundeven => q32::__lp_q32_roundeven as *const u8,
        BuiltinId::LpQ32Sin => q32::__lp_q32_sin as *const u8,
        BuiltinId::LpQ32Sinh => q32::__lp_q32_sinh as *const u8,
        BuiltinId::LpQ32Sqrt => q32::__lp_q32_sqrt as *const u8,
        BuiltinId::LpQ32Sub => q32::__lp_q32_sub as *const u8,
        BuiltinId::LpQ32Tan => q32::__lp_q32_tan as *const u8,
        BuiltinId::LpQ32Tanh => q32::__lp_q32_tanh as *const u8,
        BuiltinId::LpfxFbm2F32 => generative::fbm::fbm2_f32::__lpfx_fbm2_f32 as *const u8,
        BuiltinId::LpfxFbm2Q32 => generative::fbm::fbm2_q32::__lpfx_fbm2_q32 as *const u8,
        BuiltinId::LpfxFbm3F32 => generative::fbm::fbm3_f32::__lpfx_fbm3_f32 as *const u8,
        BuiltinId::LpfxFbm3Q32 => generative::fbm::fbm3_q32::__lpfx_fbm3_q32 as *const u8,
        BuiltinId::LpfxFbm3TileF32 => {
            generative::fbm::fbm3_tile_f32::__lpfx_fbm3_tile_f32 as *const u8
        }
        BuiltinId::LpfxFbm3TileQ32 => {
            generative::fbm::fbm3_tile_q32::__lpfx_fbm3_tile_q32 as *const u8
        }
        BuiltinId::LpfxGnoise1F32 => {
            generative::gnoise::gnoise1_f32::__lpfx_gnoise1_f32 as *const u8
        }
        BuiltinId::LpfxGnoise1Q32 => {
            generative::gnoise::gnoise1_q32::__lpfx_gnoise1_q32 as *const u8
        }
        BuiltinId::LpfxGnoise2F32 => {
            generative::gnoise::gnoise2_f32::__lpfx_gnoise2_f32 as *const u8
        }
        BuiltinId::LpfxGnoise2Q32 => {
            generative::gnoise::gnoise2_q32::__lpfx_gnoise2_q32 as *const u8
        }
        BuiltinId::LpfxGnoise3F32 => {
            generative::gnoise::gnoise3_f32::__lpfx_gnoise3_f32 as *const u8
        }
        BuiltinId::LpfxGnoise3Q32 => {
            generative::gnoise::gnoise3_q32::__lpfx_gnoise3_q32 as *const u8
        }
        BuiltinId::LpfxGnoise3TileF32 => {
            generative::gnoise::gnoise3_tile_f32::__lpfx_gnoise3_tile_f32 as *const u8
        }
        BuiltinId::LpfxGnoise3TileQ32 => {
            generative::gnoise::gnoise3_tile_q32::__lpfx_gnoise3_tile_q32 as *const u8
        }
        BuiltinId::LpfxHash1 => hash::__lpfx_hash_1 as *const u8,
        BuiltinId::LpfxHash2 => hash::__lpfx_hash_2 as *const u8,
        BuiltinId::LpfxHash3 => hash::__lpfx_hash_3 as *const u8,
        BuiltinId::LpfxHsv2rgbF32 => color::space::hsv2rgb_f32::__lpfx_hsv2rgb_f32 as *const u8,
        BuiltinId::LpfxHsv2rgbQ32 => color::space::hsv2rgb_q32::__lpfx_hsv2rgb_q32 as *const u8,
        BuiltinId::LpfxHsv2rgbVec4F32 => {
            color::space::hsv2rgb_f32::__lpfx_hsv2rgb_vec4_f32 as *const u8
        }
        BuiltinId::LpfxHsv2rgbVec4Q32 => {
            color::space::hsv2rgb_q32::__lpfx_hsv2rgb_vec4_q32 as *const u8
        }
        BuiltinId::LpfxHue2rgbF32 => color::space::hue2rgb_f32::__lpfx_hue2rgb_f32 as *const u8,
        BuiltinId::LpfxHue2rgbQ32 => color::space::hue2rgb_q32::__lpfx_hue2rgb_q32 as *const u8,
        BuiltinId::LpfxPsrdnoise2F32 => {
            generative::psrdnoise::psrdnoise2_f32::__lpfx_psrdnoise2_f32 as *const u8
        }
        BuiltinId::LpfxPsrdnoise2Q32 => {
            generative::psrdnoise::psrdnoise2_q32::__lpfx_psrdnoise2_q32 as *const u8
        }
        BuiltinId::LpfxPsrdnoise3F32 => {
            generative::psrdnoise::psrdnoise3_f32::__lpfx_psrdnoise3_f32 as *const u8
        }
        BuiltinId::LpfxPsrdnoise3Q32 => {
            generative::psrdnoise::psrdnoise3_q32::__lpfx_psrdnoise3_q32 as *const u8
        }
        BuiltinId::LpfxRandom1F32 => {
            generative::random::random1_f32::__lpfx_random1_f32 as *const u8
        }
        BuiltinId::LpfxRandom1Q32 => {
            generative::random::random1_q32::__lpfx_random1_q32 as *const u8
        }
        BuiltinId::LpfxRandom2F32 => {
            generative::random::random2_f32::__lpfx_random2_f32 as *const u8
        }
        BuiltinId::LpfxRandom2Q32 => {
            generative::random::random2_q32::__lpfx_random2_q32 as *const u8
        }
        BuiltinId::LpfxRandom3F32 => {
            generative::random::random3_f32::__lpfx_random3_f32 as *const u8
        }
        BuiltinId::LpfxRandom3Q32 => {
            generative::random::random3_q32::__lpfx_random3_q32 as *const u8
        }
        BuiltinId::LpfxRgb2hsvF32 => color::space::rgb2hsv_f32::__lpfx_rgb2hsv_f32 as *const u8,
        BuiltinId::LpfxRgb2hsvQ32 => color::space::rgb2hsv_q32::__lpfx_rgb2hsv_q32 as *const u8,
        BuiltinId::LpfxRgb2hsvVec4F32 => {
            color::space::rgb2hsv_f32::__lpfx_rgb2hsv_vec4_f32 as *const u8
        }
        BuiltinId::LpfxRgb2hsvVec4Q32 => {
            color::space::rgb2hsv_q32::__lpfx_rgb2hsv_vec4_q32 as *const u8
        }
        BuiltinId::LpfxSaturateF32 => math::saturate_f32::__lpfx_saturate_f32 as *const u8,
        BuiltinId::LpfxSaturateQ32 => math::saturate_q32::__lpfx_saturate_q32 as *const u8,
        BuiltinId::LpfxSaturateVec3F32 => math::saturate_f32::__lpfx_saturate_vec3_f32 as *const u8,
        BuiltinId::LpfxSaturateVec3Q32 => math::saturate_q32::__lpfx_saturate_vec3_q32 as *const u8,
        BuiltinId::LpfxSaturateVec4F32 => math::saturate_f32::__lpfx_saturate_vec4_f32 as *const u8,
        BuiltinId::LpfxSaturateVec4Q32 => math::saturate_q32::__lpfx_saturate_vec4_q32 as *const u8,
        BuiltinId::LpfxSnoise1F32 => {
            generative::snoise::snoise1_f32::__lpfx_snoise1_f32 as *const u8
        }
        BuiltinId::LpfxSnoise1Q32 => {
            generative::snoise::snoise1_q32::__lpfx_snoise1_q32 as *const u8
        }
        BuiltinId::LpfxSnoise2F32 => {
            generative::snoise::snoise2_f32::__lpfx_snoise2_f32 as *const u8
        }
        BuiltinId::LpfxSnoise2Q32 => {
            generative::snoise::snoise2_q32::__lpfx_snoise2_q32 as *const u8
        }
        BuiltinId::LpfxSnoise3F32 => {
            generative::snoise::snoise3_f32::__lpfx_snoise3_f32 as *const u8
        }
        BuiltinId::LpfxSnoise3Q32 => {
            generative::snoise::snoise3_q32::__lpfx_snoise3_q32 as *const u8
        }
        BuiltinId::LpfxSrandom1F32 => {
            generative::srandom::srandom1_f32::__lpfx_srandom1_f32 as *const u8
        }
        BuiltinId::LpfxSrandom1Q32 => {
            generative::srandom::srandom1_q32::__lpfx_srandom1_q32 as *const u8
        }
        BuiltinId::LpfxSrandom2F32 => {
            generative::srandom::srandom2_f32::__lpfx_srandom2_f32 as *const u8
        }
        BuiltinId::LpfxSrandom2Q32 => {
            generative::srandom::srandom2_q32::__lpfx_srandom2_q32 as *const u8
        }
        BuiltinId::LpfxSrandom3F32 => {
            generative::srandom::srandom3_f32::__lpfx_srandom3_f32 as *const u8
        }
        BuiltinId::LpfxSrandom3Q32 => {
            generative::srandom::srandom3_q32::__lpfx_srandom3_q32 as *const u8
        }
        BuiltinId::LpfxSrandom3TileF32 => {
            generative::srandom::srandom3_tile_f32::__lpfx_srandom3_tile_f32 as *const u8
        }
        BuiltinId::LpfxSrandom3TileQ32 => {
            generative::srandom::srandom3_tile_q32::__lpfx_srandom3_tile_q32 as *const u8
        }
        BuiltinId::LpfxSrandom3VecF32 => {
            generative::srandom::srandom3_vec_f32::__lpfx_srandom3_vec_f32 as *const u8
        }
        BuiltinId::LpfxSrandom3VecQ32 => {
            generative::srandom::srandom3_vec_q32::__lpfx_srandom3_vec_q32 as *const u8
        }
        BuiltinId::LpfxWorley2F32 => {
            generative::worley::worley2_f32::__lpfx_worley2_f32 as *const u8
        }
        BuiltinId::LpfxWorley2Q32 => {
            generative::worley::worley2_q32::__lpfx_worley2_q32 as *const u8
        }
        BuiltinId::LpfxWorley2ValueF32 => {
            generative::worley::worley2_value_f32::__lpfx_worley2_value_f32 as *const u8
        }
        BuiltinId::LpfxWorley2ValueQ32 => {
            generative::worley::worley2_value_q32::__lpfx_worley2_value_q32 as *const u8
        }
        BuiltinId::LpfxWorley3F32 => {
            generative::worley::worley3_f32::__lpfx_worley3_f32 as *const u8
        }
        BuiltinId::LpfxWorley3Q32 => {
            generative::worley::worley3_q32::__lpfx_worley3_q32 as *const u8
        }
        BuiltinId::LpfxWorley3ValueF32 => {
            generative::worley::worley3_value_f32::__lpfx_worley3_value_f32 as *const u8
        }
        BuiltinId::LpfxWorley3ValueQ32 => {
            generative::worley::worley3_value_q32::__lpfx_worley3_value_q32 as *const u8
        }
    }
}

/// Declare builtin functions as external symbols.
///
/// This is the same for both JIT and emulator - they both use Linkage::Import.
/// The difference is only in how they're linked:
/// - JIT: Function pointers are registered via symbol_lookup_fn during module creation
/// - Emulator: Symbols are resolved by the linker when linking the static library
///
/// `pointer_type` is the native pointer type for the target architecture.
/// For RISC-V 32-bit, this should be `types::I32`.
/// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.
pub fn declare_builtins<M: Module>(
    module: &mut M,
    pointer_type: types::Type,
) -> Result<(), GlslError> {
    for builtin in BuiltinId::all() {
        let name = builtin.name();
        let sig = builtin.signature(pointer_type);

        module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare builtin '{name}': {e}"),
                )
            })?;
    }

    Ok(())
}

/// Declare and link builtin functions for JIT mode.
///
/// This declares all builtins as external functions. The function pointers
/// are registered via a symbol lookup function that's added during module creation.
///
/// `pointer_type` is the native pointer type for the target architecture.
pub fn declare_for_jit<M: Module>(
    module: &mut M,
    pointer_type: types::Type,
) -> Result<(), GlslError> {
    declare_builtins(module, pointer_type)
}

/// Declare builtin functions as external symbols for emulator mode.
///
/// This declares all builtins as external symbols (Linkage::Import) that will
/// be resolved by the linker when linking the static library.
///
/// `pointer_type` is the native pointer type for the target architecture.
pub fn declare_for_emulator<M: Module>(
    module: &mut M,
    pointer_type: types::Type,
) -> Result<(), GlslError> {
    declare_builtins(module, pointer_type)
}
