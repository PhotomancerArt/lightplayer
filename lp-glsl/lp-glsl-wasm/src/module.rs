//! WASM module representation.

use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::GlslType;

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

pub use wasm_encoder::ValType as WasmValType;
