//! wasmtime runtime: [`WasmLpvmEngine`], [`WasmLpvmModule`], [`WasmLpvmInstance`].

mod engine;
mod instance;
pub mod link;
mod marshal;
mod native_builtin_dispatch;
mod shared_runtime;

pub use engine::{WasmLpvmEngine, WasmLpvmModule};
pub use instance::WasmLpvmInstance;

pub(crate) use shared_runtime::{WasmLpvmSharedRuntime, WasmtimeLpvmMemory};
