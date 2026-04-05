//! WASM module representation.

use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::{FloatMode, LpsType};

pub use wasm_encoder::ValType as WasmValType;

/// Export name for the shadow stack pointer global when the module uses slot memory.
pub const SHADOW_STACK_GLOBAL_EXPORT: &str = "__lp_shadow_sp";

/// Map a GLSL type to the sequence of WASM locals/results used in the ABI.
pub fn glsl_type_to_wasm_components(ty: &LpsType, float_mode: FloatMode) -> Vec<WasmValType> {
    match ty {
        LpsType::Void => Vec::new(),
        LpsType::Bool | LpsType::Int | LpsType::UInt => alloc::vec![WasmValType::I32],
        LpsType::Float => alloc::vec![scalar_float_vt(float_mode)],
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => {
            alloc::vec![component_vt(ty, float_mode); 2]
        }
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => {
            alloc::vec![component_vt(ty, float_mode); 3]
        }
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => {
            alloc::vec![component_vt(ty, float_mode); 4]
        }
        LpsType::Mat2 => alloc::vec![scalar_float_vt(float_mode); 4],
        LpsType::Mat3 => alloc::vec![scalar_float_vt(float_mode); 9],
        LpsType::Mat4 => alloc::vec![scalar_float_vt(float_mode); 16],
        LpsType::Array { element, len } => {
            let inner = glsl_type_to_wasm_components(element, float_mode);
            let mut out = Vec::with_capacity(inner.len().saturating_mul(*len as usize));
            for _ in 0..*len {
                out.extend_from_slice(&inner);
            }
            out
        }
        LpsType::Struct { members, .. } => {
            let mut out = Vec::new();
            for m in members {
                out.extend(glsl_type_to_wasm_components(&m.ty, float_mode));
            }
            out
        }
    }
}

fn scalar_float_vt(fm: FloatMode) -> WasmValType {
    match fm {
        FloatMode::Q32 => WasmValType::I32,
        FloatMode::F32 => WasmValType::F32,
    }
}

fn component_vt(ty: &LpsType, fm: FloatMode) -> WasmValType {
    match ty {
        LpsType::Vec2 | LpsType::Vec3 | LpsType::Vec4 => scalar_float_vt(fm),
        _ => WasmValType::I32,
    }
}

/// A compiled WASM module ready for instantiation.
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
    pub exports: Vec<WasmExport>,
    /// When set, WASM global index 0 is the shadow stack pointer; reset before each exported call.
    pub shadow_stack_base: Option<i32>,
}

/// Metadata for an exported WASM function.
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
    pub return_type: LpsType,
    pub param_types: Vec<LpsType>,
}
