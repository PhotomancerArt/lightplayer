//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lps-builtins-gen-app --manifest-path lp-shader/lps-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

//! Cranelift signatures and function pointers for [`BuiltinId`].
//!
//! Generated from `rust_signature` metadata scraped from `lps-builtins`.
//! Changing an `extern "C"` builtin in `lps-builtins` without re-running codegen will desync
//! this file and fail `cargo check` until you regenerate.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use lps_builtin_ids::BuiltinId;

pub(crate) fn cranelift_sig_for_builtin_inner(
    builtin: BuiltinId,
    pointer_type: types::Type,
    call_conv: CallConv,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    match builtin {
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
        | BuiltinId::LpLpirFabsQ32
        | BuiltinId::LpLpirFceilQ32
        | BuiltinId::LpLpirFfloorQ32
        | BuiltinId::LpLpirFnearestQ32
        | BuiltinId::LpLpirFsqrtQ32
        | BuiltinId::LpLpirFtoUnorm16Q32
        | BuiltinId::LpLpirFtoUnorm8Q32
        | BuiltinId::LpLpirFtoiSatSQ32
        | BuiltinId::LpLpirFtoiSatUQ32
        | BuiltinId::LpLpirFtruncQ32
        | BuiltinId::LpLpirItofSQ32
        | BuiltinId::LpLpirItofUQ32
        | BuiltinId::LpLpirUnorm16ToFQ32
        | BuiltinId::LpLpirUnorm8ToFQ32
        | BuiltinId::LpLpfnSaturateQ32
        | BuiltinId::LpVmGetFuelQ32 => {
            // extern "C" fn(i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslAtan2Q32
        | BuiltinId::LpGlslLdexpQ32
        | BuiltinId::LpGlslModQ32
        | BuiltinId::LpGlslPowQ32
        | BuiltinId::LpLpirFaddQ32
        | BuiltinId::LpLpirFdivQ32
        | BuiltinId::LpLpirFdivRecipQ32
        | BuiltinId::LpLpirFmaxQ32
        | BuiltinId::LpLpirFminQ32
        | BuiltinId::LpLpirFmulQ32
        | BuiltinId::LpLpirFsubQ32
        | BuiltinId::LpLpfnGnoise1Q32
        | BuiltinId::LpLpfnHash1
        | BuiltinId::LpLpfnRandom1Q32
        | BuiltinId::LpLpfnSnoise1Q32
        | BuiltinId::LpLpfnSrandom1Q32 => {
            // extern "C" fn(i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslFmaQ32
        | BuiltinId::LpLpfnGnoise2Q32
        | BuiltinId::LpLpfnHash2
        | BuiltinId::LpLpfnRandom2Q32
        | BuiltinId::LpLpfnSnoise2Q32
        | BuiltinId::LpLpfnSrandom2Q32
        | BuiltinId::LpLpfnWorley2Q32
        | BuiltinId::LpLpfnWorley2ValueQ32 => {
            // extern "C" fn(i32, i32, i32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpGlslSincosQ32 => {
            // extern "C" fn(i32, *mut i32, *mut i32) -> ()
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(pointer_type));
        }
        BuiltinId::LpLpfnFbm2F32 => {
            // extern "C" fn(f32, f32, i32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnFbm2Q32
        | BuiltinId::LpLpfnGnoise3Q32
        | BuiltinId::LpLpfnHash3
        | BuiltinId::LpLpfnRandom3Q32
        | BuiltinId::LpLpfnSnoise3Q32
        | BuiltinId::LpLpfnSrandom3Q32
        | BuiltinId::LpLpfnWorley3Q32
        | BuiltinId::LpLpfnWorley3ValueQ32 => {
            // extern "C" fn(i32, i32, i32, u32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnFbm3F32 => {
            // extern "C" fn(f32, f32, f32, i32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnFbm3Q32 | BuiltinId::LpLpfnGnoise3TileQ32 => {
            // extern "C" fn(i32, i32, i32, i32, u32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnFbm3TileF32 => {
            // extern "C" fn(f32, f32, f32, f32, i32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnFbm3TileQ32 => {
            // extern "C" fn(i32, i32, i32, i32, i32, u32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnGnoise1F32
        | BuiltinId::LpLpfnRandom1F32
        | BuiltinId::LpLpfnSnoise1F32
        | BuiltinId::LpLpfnSrandom1F32 => {
            // extern "C" fn(f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnGnoise2F32
        | BuiltinId::LpLpfnRandom2F32
        | BuiltinId::LpLpfnSnoise2F32
        | BuiltinId::LpLpfnSrandom2F32
        | BuiltinId::LpLpfnWorley2F32
        | BuiltinId::LpLpfnWorley2ValueF32 => {
            // extern "C" fn(f32, f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnGnoise3F32
        | BuiltinId::LpLpfnRandom3F32
        | BuiltinId::LpLpfnSnoise3F32
        | BuiltinId::LpLpfnSrandom3F32
        | BuiltinId::LpLpfnWorley3F32
        | BuiltinId::LpLpfnWorley3ValueF32 => {
            // extern "C" fn(f32, f32, f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnGnoise3TileF32 => {
            // extern "C" fn(f32, f32, f32, f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnHsv2rgbF32
        | BuiltinId::LpLpfnRgb2hsvF32
        | BuiltinId::LpLpfnSaturateVec3F32 => {
            // extern "C" fn(*mut f32, f32, f32, f32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnHsv2rgbQ32
        | BuiltinId::LpLpfnRgb2hsvQ32
        | BuiltinId::LpLpfnSaturateVec3Q32 => {
            // extern "C" fn(*mut i32, i32, i32, i32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnHsv2rgbVec4F32
        | BuiltinId::LpLpfnRgb2hsvVec4F32
        | BuiltinId::LpLpfnSaturateVec4F32 => {
            // extern "C" fn(*mut f32, f32, f32, f32, f32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnHsv2rgbVec4Q32
        | BuiltinId::LpLpfnRgb2hsvVec4Q32
        | BuiltinId::LpLpfnSaturateVec4Q32
        | BuiltinId::LpLpfnSrandom3VecQ32 => {
            // extern "C" fn(*mut i32, i32, i32, i32, i32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnHue2rgbF32 => {
            // extern "C" fn(*mut f32, f32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnHue2rgbQ32 => {
            // extern "C" fn(*mut i32, i32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnPsrdnoise2F32 => {
            // extern "C" fn(f32, f32, f32, f32, f32, *mut f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnPsrdnoise2Q32 => {
            // extern "C" fn(i32, i32, i32, i32, i32, *mut i32, u32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnPsrdnoise3F32 => {
            // extern "C" fn(f32, f32, f32, f32, f32, f32, f32, *mut f32, u32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnPsrdnoise3Q32 => {
            // extern "C" fn(i32, i32, i32, i32, i32, i32, i32, *mut i32, u32) -> i32
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnSaturateF32 => {
            // extern "C" fn(f32) -> f32
            sig.params.push(AbiParam::new(types::F32));
            sig.returns.push(AbiParam::new(types::F32));
        }
        BuiltinId::LpLpfnSrandom3TileF32 => {
            // extern "C" fn(*mut f32, f32, f32, f32, f32, u32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnSrandom3TileQ32 => {
            // extern "C" fn(*mut i32, i32, i32, i32, i32, u32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpLpfnSrandom3VecF32 => {
            // extern "C" fn(*mut f32, f32, f32, f32, u32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::F32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpTexTexture1dR16UnormQ32 | BuiltinId::LpTexTexture1dRgba16UnormQ32 => {
            // unsafe extern "C" fn(*mut i32, u32, u32, u32, i32, u32, u32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
        }
        BuiltinId::LpTexTexture2dR16UnormQ32 | BuiltinId::LpTexTexture2dRgba16UnormQ32 => {
            // unsafe extern "C" fn(*mut i32, u32, u32, u32, u32, i32, i32, u32, u32, u32) -> ()
            sig.params.push(AbiParam::new(pointer_type));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
        }
    }
    sig
}
