//! WASM module representation and GLSL type → WASM component mapping.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_shared::LpsType;

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

/// `env.memory` import limits (matches [`emit::emit_module`](crate::emit::emit_module)).
#[derive(Debug, Clone, Copy)]
pub struct EnvMemorySpec {
    pub initial_pages: u32,
    pub max_pages: Option<u32>,
}

impl EnvMemorySpec {
    /// WebAssembly page size in bytes (64 KiB). Keep aligned with emit and JS descriptors.
    pub const WASM_PAGE_SIZE: u32 = 64 * 1024;

    /// Guest-reserved bytes at the start of linear memory (one page). Host [`LpvmMemory::alloc`]
    /// bumps above this so low addresses stay available for shadow stack / guest data.
    pub const fn guest_reserve_bytes() -> u32 {
        Self::WASM_PAGE_SIZE
    }

    /// Limits recorded on emitted modules when they import `env.memory` (minimum 1 page, no max).
    #[inline]
    pub const fn shader_import_limits() -> Self {
        Self {
            initial_pages: 1,
            max_pages: None,
        }
    }

    /// Engine-owned linear memory at startup: satisfies shader import minimum and reserves the
    /// first page for guest use before the host bump region.
    #[inline]
    pub const fn engine_initial_for_host() -> Self {
        Self {
            initial_pages: 2,
            max_pages: None,
        }
    }
}

/// A compiled WASM module ready for instantiation.
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
    pub exports: Vec<WasmExport>,
    /// When set, WASM global index 0 is the shadow stack pointer; reset before each exported call.
    pub shadow_stack_base: Option<i32>,
    /// When set, the module imports `env.memory`; browsers/wasmtime must supply matching limits.
    pub env_memory: Option<EnvMemorySpec>,
}

/// Metadata for an exported WASM function.
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
    pub return_type: LpsType,
    pub param_types: Vec<LpsType>,
    /// `IrFunction::sret_arg` — aggregate (etc.) return via hidden pointer param; wasm has no results.
    pub uses_sret: bool,
}

impl WasmModule {
    /// Raw WASM bytes (e.g. for the browser or `wasmtime::Module::new`).
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
