//! wasmtime runtime: [`WasmLpvmEngine`], [`WasmLpvmModule`], [`WasmLpvmInstance`].

mod engine;
mod instance;
pub mod link;
mod marshal;
mod native_builtin_dispatch;

pub use engine::{WasmLpvmEngine, WasmLpvmModule};
pub use instance::WasmLpvmInstance;
