//! LPFX function registry
//!
//! This module contains the array of all LPFX functions.
//! This file is AUTO-GENERATED. Do not edit manually.
//!
//! To regenerate this file, run:
//!     cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
//!
//! Or use the build script:
//!     scripts/build-builtins.sh

use super::lpfx_fn::{LpfxFn, LpfxFnImpl};
use crate::backend::builtins::registry::BuiltinId;
use crate::semantic::functions::{FunctionSignature, ParamQualifier, Parameter};
use crate::semantic::types::Type;
use alloc::{boxed::Box, string::String, vec, vec::Vec};

/// Registry of all LPFX functions
///
/// This is the single source of truth for all LPFX function definitions.
/// Functions are looked up by name from this array.
///
/// Returns a static reference to avoid recreating the Vec on every call.
pub fn lpfx_fns() -> &'static [LpfxFn] {
    #[cfg(feature = "std")]
    {
        static FUNCTIONS: std::sync::OnceLock<&'static [LpfxFn]> = std::sync::OnceLock::new();
        *FUNCTIONS.get_or_init(|| init_functions())
    }
    #[cfg(not(feature = "std"))]
    {
        // In no_std, use a static initialized on first access
        // This is safe because the data is immutable after initialization
        static mut FUNCTIONS: Option<&'static [LpfxFn]> = None;
        unsafe {
            let ptr = core::ptr::addr_of_mut!(FUNCTIONS);
            if (*ptr).is_none() {
                *ptr = Some(init_functions());
            }
            (*ptr).unwrap_unchecked()
        }
    }
}

fn init_functions() -> &'static [LpfxFn] {
    let vec: Vec<LpfxFn> = vec![
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_fbm"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("octaves"),
                        ty: Type::Int,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxFbm2F32,
                q32_impl: BuiltinId::LpfxFbm2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_fbm"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("octaves"),
                        ty: Type::Int,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxFbm3F32,
                q32_impl: BuiltinId::LpfxFbm3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_fbm"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("tileLength"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("octaves"),
                        ty: Type::Int,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxFbm3TileF32,
                q32_impl: BuiltinId::LpfxFbm3TileQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_gnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxGnoise1F32,
                q32_impl: BuiltinId::LpfxGnoise1Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_gnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxGnoise2F32,
                q32_impl: BuiltinId::LpfxGnoise2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_gnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxGnoise3F32,
                q32_impl: BuiltinId::LpfxGnoise3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_gnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("tileLength"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxGnoise3TileF32,
                q32_impl: BuiltinId::LpfxGnoise3TileQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hash"),
                return_type: Type::UInt,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash1),
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hash"),
                return_type: Type::UInt,
                parameters: vec![
                    Parameter {
                        name: String::from("xy"),
                        ty: Type::UVec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash2),
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hash"),
                return_type: Type::UInt,
                parameters: vec![
                    Parameter {
                        name: String::from("xyz"),
                        ty: Type::UVec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash3),
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hsv2rgb"),
                return_type: Type::Vec3,
                parameters: vec![Parameter {
                    name: String::from("hsv"),
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxHsv2rgbF32,
                q32_impl: BuiltinId::LpfxHsv2rgbQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hsv2rgb"),
                return_type: Type::Vec4,
                parameters: vec![Parameter {
                    name: String::from("hsv"),
                    ty: Type::Vec4,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxHsv2rgbVec4F32,
                q32_impl: BuiltinId::LpfxHsv2rgbVec4Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_hue2rgb"),
                return_type: Type::Vec3,
                parameters: vec![Parameter {
                    name: String::from("hue"),
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxHue2rgbF32,
                q32_impl: BuiltinId::LpfxHue2rgbQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_psrdnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("period"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("alpha"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("gradient"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::Out,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxPsrdnoise2F32,
                q32_impl: BuiltinId::LpfxPsrdnoise2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_psrdnoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("period"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("alpha"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("gradient"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::Out,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxPsrdnoise3F32,
                q32_impl: BuiltinId::LpfxPsrdnoise3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_random"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxRandom1F32,
                q32_impl: BuiltinId::LpfxRandom1Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_random"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxRandom2F32,
                q32_impl: BuiltinId::LpfxRandom2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_random"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxRandom3F32,
                q32_impl: BuiltinId::LpfxRandom3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_rgb2hsv"),
                return_type: Type::Vec3,
                parameters: vec![Parameter {
                    name: String::from("rgb"),
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxRgb2hsvF32,
                q32_impl: BuiltinId::LpfxRgb2hsvQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_rgb2hsv"),
                return_type: Type::Vec4,
                parameters: vec![Parameter {
                    name: String::from("rgb"),
                    ty: Type::Vec4,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxRgb2hsvVec4F32,
                q32_impl: BuiltinId::LpfxRgb2hsvVec4Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_saturate"),
                return_type: Type::Float,
                parameters: vec![Parameter {
                    name: String::from("x"),
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSaturateF32,
                q32_impl: BuiltinId::LpfxSaturateQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_saturate"),
                return_type: Type::Vec3,
                parameters: vec![Parameter {
                    name: String::from("v"),
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSaturateVec3F32,
                q32_impl: BuiltinId::LpfxSaturateVec3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_saturate"),
                return_type: Type::Vec4,
                parameters: vec![Parameter {
                    name: String::from("v"),
                    ty: Type::Vec4,
                    qualifier: ParamQualifier::In,
                }],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSaturateVec4F32,
                q32_impl: BuiltinId::LpfxSaturateVec4Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_snoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSnoise1F32,
                q32_impl: BuiltinId::LpfxSnoise1Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_snoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSnoise2F32,
                q32_impl: BuiltinId::LpfxSnoise2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_snoise"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSnoise3F32,
                q32_impl: BuiltinId::LpfxSnoise3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_srandom"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("x"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSrandom1F32,
                q32_impl: BuiltinId::LpfxSrandom1Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_srandom"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSrandom2F32,
                q32_impl: BuiltinId::LpfxSrandom2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_srandom"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSrandom3F32,
                q32_impl: BuiltinId::LpfxSrandom3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_srandom3_tile"),
                return_type: Type::Vec3,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("tileLength"),
                        ty: Type::Float,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSrandom3TileF32,
                q32_impl: BuiltinId::LpfxSrandom3TileQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_srandom3_vec"),
                return_type: Type::Vec3,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxSrandom3VecF32,
                q32_impl: BuiltinId::LpfxSrandom3VecQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_worley"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxWorley2F32,
                q32_impl: BuiltinId::LpfxWorley2Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_worley"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxWorley3F32,
                q32_impl: BuiltinId::LpfxWorley3Q32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_worley_value"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec2,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxWorley2ValueF32,
                q32_impl: BuiltinId::LpfxWorley2ValueQ32,
            },
        },
        LpfxFn {
            glsl_sig: FunctionSignature {
                name: String::from("lpfx_worley_value"),
                return_type: Type::Float,
                parameters: vec![
                    Parameter {
                        name: String::from("p"),
                        ty: Type::Vec3,
                        qualifier: ParamQualifier::In,
                    },
                    Parameter {
                        name: String::from("seed"),
                        ty: Type::UInt,
                        qualifier: ParamQualifier::In,
                    },
                ],
            },
            impls: LpfxFnImpl::Decimal {
                float_impl: BuiltinId::LpfxWorley3ValueF32,
                q32_impl: BuiltinId::LpfxWorley3ValueQ32,
            },
        },
    ];
    Box::leak(vec.into_boxed_slice())
}
