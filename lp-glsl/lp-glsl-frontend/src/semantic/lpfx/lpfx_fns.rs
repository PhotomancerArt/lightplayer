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

use super::lpfx_fn::{FunctionSignatureRef, LpfxFn, LpfxFnImpl, ParameterRef};
use crate::semantic::functions::ParamQualifier;
use crate::semantic::types::Type;
use lp_glsl_builtin_ids::BuiltinId;

static LPFX_FNS: &[LpfxFn] = &[
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_fbm",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "octaves",
                    ty: Type::Int,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_fbm",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "octaves",
                    ty: Type::Int,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_fbm",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "tileLength",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "octaves",
                    ty: Type::Int,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_gnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_gnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_gnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_gnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "tileLength",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hash",
            return_type: Type::UInt,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
            ],
        },
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash1),
    },
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hash",
            return_type: Type::UInt,
            parameters: &[
                ParameterRef {
                    name: "xy",
                    ty: Type::UVec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
            ],
        },
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash2),
    },
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hash",
            return_type: Type::UInt,
            parameters: &[
                ParameterRef {
                    name: "xyz",
                    ty: Type::UVec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
            ],
        },
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpfxHash3),
    },
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hsv2rgb",
            return_type: Type::Vec3,
            parameters: &[ParameterRef {
                name: "hsv",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hsv2rgb",
            return_type: Type::Vec4,
            parameters: &[ParameterRef {
                name: "hsv",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_hue2rgb",
            return_type: Type::Vec3,
            parameters: &[ParameterRef {
                name: "hue",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_psrdnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "period",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "alpha",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "gradient",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::Out,
                },
                ParameterRef {
                    name: "seed",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
            ],
        },
        impls: LpfxFnImpl::Decimal {
            float_impl: BuiltinId::LpfxPsrdnoise2F32,
            q32_impl: BuiltinId::LpfxPsrdnoise2Q32,
        },
    },
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_psrdnoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "period",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "alpha",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "gradient",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::Out,
                },
                ParameterRef {
                    name: "seed",
                    ty: Type::UInt,
                    qualifier: ParamQualifier::In,
                },
            ],
        },
        impls: LpfxFnImpl::Decimal {
            float_impl: BuiltinId::LpfxPsrdnoise3F32,
            q32_impl: BuiltinId::LpfxPsrdnoise3Q32,
        },
    },
    LpfxFn {
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_random",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_random",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_random",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_rgb2hsv",
            return_type: Type::Vec3,
            parameters: &[ParameterRef {
                name: "rgb",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_rgb2hsv",
            return_type: Type::Vec4,
            parameters: &[ParameterRef {
                name: "rgb",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_saturate",
            return_type: Type::Float,
            parameters: &[ParameterRef {
                name: "x",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_saturate",
            return_type: Type::Vec3,
            parameters: &[ParameterRef {
                name: "v",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_saturate",
            return_type: Type::Vec4,
            parameters: &[ParameterRef {
                name: "v",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_snoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_snoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_snoise",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_srandom",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "x",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_srandom",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_srandom",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_srandom3_tile",
            return_type: Type::Vec3,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "tileLength",
                    ty: Type::Float,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_srandom3_vec",
            return_type: Type::Vec3,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_worley",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_worley",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_worley_value",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec2,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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
        glsl_sig: FunctionSignatureRef {
            name: "lpfx_worley_value",
            return_type: Type::Float,
            parameters: &[
                ParameterRef {
                    name: "p",
                    ty: Type::Vec3,
                    qualifier: ParamQualifier::In,
                },
                ParameterRef {
                    name: "seed",
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

/// Registry of all LPFX functions
///
/// This is the single source of truth for all LPFX function definitions.
/// Functions are looked up by name from this array.
///
/// Returns a static reference. Data lives in .rodata (no heap allocations).
pub fn lpfx_fns() -> &'static [LpfxFn] {
    LPFX_FNS
}
