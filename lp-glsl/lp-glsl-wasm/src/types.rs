//! GLSL type metadata to WASM [`ValType`] mapping.

use alloc::vec::Vec;

use lp_glsl_naga::{FloatMode, GlslType};
use naga::{Handle, Module, ScalarKind, Type, TypeInner};
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

/// How many scalar WASM values this Naga type occupies (scalar = 1, vec3 = 3).
pub fn type_handle_component_count(module: &Module, ty: Handle<Type>) -> u32 {
    type_inner_component_count(module, &module.types[ty].inner)
}

fn type_inner_component_count(module: &Module, inner: &TypeInner) -> u32 {
    match *inner {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => size as u32,
        TypeInner::Pointer { base, .. } => type_handle_component_count(module, base),
        TypeInner::ValuePointer {
            size: Some(vector_size),
            ..
        } => vector_size as u32,
        TypeInner::ValuePointer { size: None, .. } => 1,
        TypeInner::Matrix { columns, rows, .. } => (columns as u32) * (rows as u32),
        _ => 1,
    }
}

/// Scalar kind of vector/matrix element (or the scalar itself).
pub fn type_handle_element_scalar_kind(
    module: &Module,
    ty: Handle<Type>,
) -> Result<ScalarKind, &'static str> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Vector { scalar, .. } | TypeInner::Matrix { scalar, .. } => Ok(scalar.kind),
        TypeInner::Pointer { base, .. } => type_handle_element_scalar_kind(module, *base),
        TypeInner::ValuePointer { scalar, .. } => Ok(scalar.kind),
        _ => Err("type has no element scalar kind"),
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
        TypeInner::Vector { scalar, .. } => match scalar.kind {
            ScalarKind::Float if scalar.width == 4 => scalar_float_vt(mode),
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool if scalar.width == 4 => {
                ValType::I32
            }
            _ => ValType::I32,
        },
        _ => ValType::I32,
    }
}
