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
            BuiltinId::LpGlslAcosQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAcoshQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAsinQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAsinhQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAtan2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAtanQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslAtanhQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslCosQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslCoshQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslExp2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslExpQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslFmaQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslInversesqrtQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslLdexpQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslLog2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslLogQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslModQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslPowQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslRoundQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslSinQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslSinhQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslTanQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpGlslTanhQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFaddQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFdivQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFmulQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFnearestQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFsqrtQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpirFsubQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxFbm2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxFbm2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxFbm3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxFbm3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxFbm3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxFbm3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxGnoise1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxGnoise1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxGnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxGnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxGnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxGnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxGnoise3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxGnoise3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxHash1 => None,
            BuiltinId::LpLpfxHash2 => None,
            BuiltinId::LpLpfxHash3 => None,
            BuiltinId::LpLpfxHsv2rgbF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxHsv2rgbQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxHsv2rgbVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxHsv2rgbVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxHue2rgbF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxHue2rgbQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxPsrdnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxPsrdnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxPsrdnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxPsrdnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxRandom1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxRandom1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxRandom2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxRandom2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxRandom3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxRandom3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxRgb2hsvF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxRgb2hsvQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxRgb2hsvVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxRgb2hsvVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSaturateF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSaturateQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSaturateVec3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSaturateVec3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSaturateVec4F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSaturateVec4Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSnoise1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSnoise1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSnoise2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSnoise2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSnoise3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSnoise3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSrandom1F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSrandom1Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSrandom2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSrandom2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSrandom3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSrandom3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSrandom3TileF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSrandom3TileQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxSrandom3VecF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxSrandom3VecQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxWorley2F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxWorley2Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxWorley2ValueF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxWorley2ValueQ32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxWorley3F32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxWorley3Q32 => Some(crate::FloatMode::Q32),
            BuiltinId::LpLpfxWorley3ValueF32 => Some(crate::FloatMode::Float),
            BuiltinId::LpLpfxWorley3ValueQ32 => Some(crate::FloatMode::Q32),
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

/// Get function pointer for a builtin (JIT mode only).
///
/// Returns the function pointer that can be registered with JITModule.
pub fn get_function_pointer(builtin: BuiltinId) -> *const u8 {
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
