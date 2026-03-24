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

pub use lp_glsl_builtin_ids::BuiltinId;

use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{Linkage, Module};

#[cfg(not(feature = "std"))]
use alloc::format;

/// Format affinity for builtins (Cranelift-specific, format-aware declaration).
trait BuiltinIdFormat {
    fn format(&self) -> Option<crate::FloatMode>;
}

impl BuiltinIdFormat for BuiltinId {
    fn format(&self) -> Option<crate::FloatMode> {
        match self {
            BuiltinId::LpQ32Acos => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Acosh => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Add => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Asin => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Asinh => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Atan => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Atan2 => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Atanh => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Cos => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Cosh => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Div => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Exp => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Exp2 => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Fma => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Inversesqrt => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Ldexp => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Log => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Log2 => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Mod => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Mul => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Pow => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Round => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Roundeven => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Sin => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Sinh => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Sqrt => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Sub => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Tan => Some(crate::FloatMode::Q32),
            BuiltinId::LpQ32Tanh => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxFbm2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxFbm2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxFbm3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxFbm3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxFbm3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxFbm3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxGnoise1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxGnoise1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxGnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxGnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxGnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxGnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxGnoise3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxGnoise3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxHash1 => None,
            BuiltinId::LpfxHash2 => None,
            BuiltinId::LpfxHash3 => None,
            BuiltinId::LpfxHsv2rgbF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxHsv2rgbQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxHsv2rgbVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxHsv2rgbVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxHue2rgbF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxHue2rgbQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxPsrdnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxPsrdnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxPsrdnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxPsrdnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxRandom1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxRandom1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxRandom2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxRandom2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxRandom3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxRandom3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxRgb2hsvF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxRgb2hsvQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxRgb2hsvVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxRgb2hsvVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSaturateF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSaturateQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSaturateVec3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSaturateVec3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSaturateVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSaturateVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSnoise1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSnoise1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSrandom1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSrandom1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSrandom2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSrandom2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSrandom3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSrandom3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSrandom3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSrandom3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxSrandom3VecF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxSrandom3VecQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxWorley2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxWorley2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxWorley2ValueF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxWorley2ValueQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxWorley3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxWorley3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpfxWorley3ValueF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpfxWorley3ValueQ32 => Some(crate::FloatMode::Q32),
        }
    }
}

/// Get the Cranelift signature for this builtin function.
///
/// `pointer_type` is the native pointer type for the target architecture.
/// For RISC-V 32-bit, this should be `types::I32`.
/// For 64-bit architectures (like Apple Silicon), this should be `types::I64`.
pub fn signature_for_builtin(builtin: BuiltinId, pointer_type: types::Type) -> Signature {
    let mut sig = Signature::new(CallConv::SystemV);
    match builtin {
        BuiltinId::LpfxPsrdnoise2F32 | BuiltinId::LpfxPsrdnoise2Q32 => {
            // Matches `__lpfx_psrdnoise2_*`: x, y, period_x, period_y, alpha, gradient_out, seed
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32)); // uint seed (zero-extended)
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpfxPsrdnoise3F32 | BuiltinId::LpfxPsrdnoise3Q32 => {
            // Matches `__lpfx_psrdnoise3_*`: x,y,z, period_*, alpha, gradient_out, seed
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32)); // uint seed (zero-extended)
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
/// `format` filters builtins: in Q32 mode, F32-only builtins are skipped; in Float mode, Q32 builtins are skipped.
pub fn declare_builtins<M: Module>(
    module: &mut M,
    pointer_type: types::Type,
    format: crate::FloatMode,
) -> Result<(), GlslError> {
    for builtin in BuiltinId::all() {
        if let Some(f) = builtin.format() {
            if f != format {
                continue;
            }
        }
        let name = builtin.name();
        let sig = signature_for_builtin(*builtin, pointer_type);

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
    format: crate::FloatMode,
) -> Result<(), GlslError> {
    declare_builtins(module, pointer_type, format)
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
    format: crate::FloatMode,
) -> Result<(), GlslError> {
    declare_builtins(module, pointer_type, format)
}
