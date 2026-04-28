//! LPIR → WebAssembly emission for LPVM, plus host runtimes.
//!
//! Primary entry point: [`compile_lpir`]. On native targets, [`rt_wasmtime::WasmLpvmEngine`]
//! implements [`lpvm::LpvmEngine`] using wasmtime.

extern crate alloc;

mod aggregate_abi;
mod compile;
mod emit;
pub mod error;
pub mod module;
pub mod options;

#[cfg(target_arch = "wasm32")]
pub mod rt_browser;
#[cfg(not(target_arch = "wasm32"))]
pub mod rt_wasmtime;

pub use compile::{WasmArtifact, compile_lpir};
pub use error::WasmError;
pub use lpir::FloatMode;
pub use module::{
    SHADOW_STACK_GLOBAL_EXPORT, WasmExport, WasmModule, WasmValType, glsl_type_to_wasm_components,
};
pub use options::WasmOptions;
