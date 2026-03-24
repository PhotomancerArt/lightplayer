//! WASM module representation.

use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::{FloatMode, GlslType};

pub use wasm_encoder::ValType as WasmValType;

/// Map a GLSL type to the sequence of WASM locals/results used in the ABI.
pub fn glsl_type_to_wasm_components(ty: &GlslType, float_mode: FloatMode) -> Vec<WasmValType> {
    match ty {
        GlslType::Void => Vec::new(),
        GlslType::Bool | GlslType::Int | GlslType::UInt => alloc::vec![WasmValType::I32],
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

fn scalar_float_vt(fm: FloatMode) -> WasmValType {
    match fm {
        FloatMode::Q32 => WasmValType::I32,
        FloatMode::Float => WasmValType::F32,
    }
}

fn component_vt(ty: &GlslType, fm: FloatMode) -> WasmValType {
    match ty {
        GlslType::Vec2 | GlslType::Vec3 | GlslType::Vec4 => scalar_float_vt(fm),
        _ => WasmValType::I32,
    }
}

/// A compiled WASM module ready for instantiation.
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
    pub exports: Vec<WasmExport>,
}

/// Metadata for an exported WASM function.
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
    pub return_type: GlslType,
    pub param_types: Vec<GlslType>,
}
