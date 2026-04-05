//! LPIR → WebAssembly emission for LPVM, with a wasmtime runtime enabled by default.
//!
//! Primary entry point: [`compile_lpir`]. [`runtime::WasmLpvmEngine`] implements [`lpvm::LpvmEngine`].
//!
//! **Emit-only (no wasmtime):** depend on this crate with `default-features = false` (keeps
//! `#![no_std]`). The `runtime` feature can be enabled explicitly when needed.

#![cfg_attr(not(feature = "runtime"), no_std)]

extern crate alloc;

mod compile;
mod emit;
pub mod error;
pub mod module;
pub mod options;

#[cfg(feature = "runtime")]
pub mod runtime;

pub use compile::{WasmArtifact, compile_lpir};
pub use error::WasmError;
pub use lpir::FloatMode;
pub use module::{
    SHADOW_STACK_GLOBAL_EXPORT, WasmExport, WasmModule, WasmValType, glsl_type_to_wasm_components,
};
pub use options::WasmOptions;
