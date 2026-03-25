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
            float_impl: BuiltinId::LpLpfxFbm2F32,
            q32_impl: BuiltinId::LpLpfxFbm2Q32,
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
            float_impl: BuiltinId::LpLpfxFbm3F32,
            q32_impl: BuiltinId::LpLpfxFbm3Q32,
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
            float_impl: BuiltinId::LpLpfxFbm3TileF32,
            q32_impl: BuiltinId::LpLpfxFbm3TileQ32,
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
            float_impl: BuiltinId::LpLpfxGnoise1F32,
            q32_impl: BuiltinId::LpLpfxGnoise1Q32,
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
            float_impl: BuiltinId::LpLpfxGnoise2F32,
            q32_impl: BuiltinId::LpLpfxGnoise2Q32,
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
            float_impl: BuiltinId::LpLpfxGnoise3F32,
            q32_impl: BuiltinId::LpLpfxGnoise3Q32,
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
            float_impl: BuiltinId::LpLpfxGnoise3TileF32,
            q32_impl: BuiltinId::LpLpfxGnoise3TileQ32,
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
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpLpfxHash1),
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
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpLpfxHash2),
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
        impls: LpfxFnImpl::NonDecimal(BuiltinId::LpLpfxHash3),
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
            float_impl: BuiltinId::LpLpfxHsv2rgbF32,
            q32_impl: BuiltinId::LpLpfxHsv2rgbQ32,
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
            float_impl: BuiltinId::LpLpfxHsv2rgbVec4F32,
            q32_impl: BuiltinId::LpLpfxHsv2rgbVec4Q32,
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
            float_impl: BuiltinId::LpLpfxHue2rgbF32,
            q32_impl: BuiltinId::LpLpfxHue2rgbQ32,
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
            float_impl: BuiltinId::LpLpfxPsrdnoise2F32,
            q32_impl: BuiltinId::LpLpfxPsrdnoise2Q32,
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
            float_impl: BuiltinId::LpLpfxPsrdnoise3F32,
            q32_impl: BuiltinId::LpLpfxPsrdnoise3Q32,
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
            float_impl: BuiltinId::LpLpfxRandom1F32,
            q32_impl: BuiltinId::LpLpfxRandom1Q32,
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
            float_impl: BuiltinId::LpLpfxRandom2F32,
            q32_impl: BuiltinId::LpLpfxRandom2Q32,
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
            float_impl: BuiltinId::LpLpfxRandom3F32,
            q32_impl: BuiltinId::LpLpfxRandom3Q32,
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
            float_impl: BuiltinId::LpLpfxRgb2hsvF32,
            q32_impl: BuiltinId::LpLpfxRgb2hsvQ32,
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
            float_impl: BuiltinId::LpLpfxRgb2hsvVec4F32,
            q32_impl: BuiltinId::LpLpfxRgb2hsvVec4Q32,
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
            float_impl: BuiltinId::LpLpfxSaturateF32,
            q32_impl: BuiltinId::LpLpfxSaturateQ32,
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
            float_impl: BuiltinId::LpLpfxSaturateVec3F32,
            q32_impl: BuiltinId::LpLpfxSaturateVec3Q32,
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
            float_impl: BuiltinId::LpLpfxSaturateVec4F32,
            q32_impl: BuiltinId::LpLpfxSaturateVec4Q32,
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
            float_impl: BuiltinId::LpLpfxSnoise1F32,
            q32_impl: BuiltinId::LpLpfxSnoise1Q32,
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
            float_impl: BuiltinId::LpLpfxSnoise2F32,
            q32_impl: BuiltinId::LpLpfxSnoise2Q32,
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
            float_impl: BuiltinId::LpLpfxSnoise3F32,
            q32_impl: BuiltinId::LpLpfxSnoise3Q32,
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
            float_impl: BuiltinId::LpLpfxSrandom1F32,
            q32_impl: BuiltinId::LpLpfxSrandom1Q32,
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
            float_impl: BuiltinId::LpLpfxSrandom2F32,
            q32_impl: BuiltinId::LpLpfxSrandom2Q32,
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
            float_impl: BuiltinId::LpLpfxSrandom3F32,
            q32_impl: BuiltinId::LpLpfxSrandom3Q32,
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
            float_impl: BuiltinId::LpLpfxSrandom3TileF32,
            q32_impl: BuiltinId::LpLpfxSrandom3TileQ32,
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
            float_impl: BuiltinId::LpLpfxSrandom3VecF32,
            q32_impl: BuiltinId::LpLpfxSrandom3VecQ32,
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
            float_impl: BuiltinId::LpLpfxWorley2F32,
            q32_impl: BuiltinId::LpLpfxWorley2Q32,
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
            float_impl: BuiltinId::LpLpfxWorley3F32,
            q32_impl: BuiltinId::LpLpfxWorley3Q32,
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
            float_impl: BuiltinId::LpLpfxWorley2ValueF32,
            q32_impl: BuiltinId::LpLpfxWorley2ValueQ32,
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
            float_impl: BuiltinId::LpLpfxWorley3ValueF32,
            q32_impl: BuiltinId::LpLpfxWorley3ValueQ32,
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
