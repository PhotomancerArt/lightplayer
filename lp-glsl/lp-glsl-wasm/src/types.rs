//! GLSL type metadata to WASM [`ValType`] mapping.

use alloc::vec::Vec;

use lp_glsl_naga::{FloatMode, GlslType};
use naga::{ScalarKind, TypeInner};
use wasm_encoder::ValType;

pub fn glsl_type_to_wasm_components(ty: &GlslType, float_mode: FloatMode) -> Vec<ValType> {
    match ty {
        GlslType::Void => alloc::vec![],
        GlslType::Bool | GlslType::Int | GlslType::UInt => alloc::vec![ValType::I32],
        GlslType::Float => alloc::vec![scalar_float_vt(float_mode)],
        GlslType::Vec2 | GlslType::IVec2 | GlslType::UVec2 | GlslType::BVec2 => {
            alloc::vec![component_vt(ty, float_mode); 2]
        }
        GlslType::Vec3 | GlslType::IVec3 | GlslType::UVec3 | GlslType::BVec3 => {
            alloc::vec![component_vt(ty, float_mode); 3]
        }
        GlslType::Vec4 | GlslType::IVec4 | GlslType::UVec4 | GlslType::BVec4 => {
            alloc::vec![component_vt(ty, float_mode); 4]
        }
    }
}

fn scalar_float_vt(fm: FloatMode) -> ValType {
    match fm {
        FloatMode::Q32 => ValType::I32,
        FloatMode::Float => ValType::F32,
    }
}

fn component_vt(ty: &GlslType, fm: FloatMode) -> ValType {
    match ty {
        GlslType::Vec2 | GlslType::Vec3 | GlslType::Vec4 => scalar_float_vt(fm),
        _ => ValType::I32,
    }
}

/// Map a Naga scalar (pointer target) type to the WASM local type used to hold its value.
pub fn scalar_naga_inner_to_valtype(inner: &TypeInner, mode: FloatMode) -> ValType {
    match *inner {
        TypeInner::Scalar(s) => match s.kind {
            ScalarKind::Float if s.width == 4 => scalar_float_vt(mode),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool if s.width == 4 => ValType::I32,
            _ => ValType::I32,
        },
        _ => ValType::I32,
    }
}
