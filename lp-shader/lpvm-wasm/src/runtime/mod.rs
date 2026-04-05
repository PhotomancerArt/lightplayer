//! wasmtime runtime: [`WasmLpvmEngine`], [`WasmLpvmModule`], [`WasmLpvmInstance`].
//!
//! Requires the `runtime` crate feature (pulls in `wasmtime` and `std`).

mod engine;
mod instance;
pub mod link;
mod marshal;

pub use engine::{WasmLpvmEngine, WasmLpvmModule};
pub use instance::WasmLpvmInstance;
