//! WASM module representation.

use alloc::string::String;
use alloc::vec::Vec;
use lp_glsl_frontend::semantic::functions::FunctionSignature;

/// A compiled WASM module ready for instantiation.
#[derive(Debug, Clone)]
pub struct WasmModule {
    /// Raw WASM binary bytes, ready for WebAssembly.instantiate() or wasmtime.
    pub bytes: Vec<u8>,
    /// Exported function names and their signatures.
    pub exports: Vec<WasmExport>,
}

/// Metadata for an exported WASM function.
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
    /// GLSL function signature for execute_function dispatch.
    pub signature: FunctionSignature,
}

/// WASM value type (re-export from wasm-encoder for convenience).
pub use wasm_encoder::ValType as WasmValType;
