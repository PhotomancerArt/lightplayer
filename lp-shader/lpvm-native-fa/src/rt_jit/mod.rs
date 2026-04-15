//! JIT compilation for RISC-V targets (`no_std` + `alloc`).
//!
//! Emits the same RV32 machine code as the ELF path, then patches auipc+jalr pairs against a
//! [`BuiltinTable`] and in-image function symbols. Intended for `fw-esp32` / `fw-emu` to avoid
//! ELF link overhead.

mod buffer;
mod builtins;
mod call;
mod compiler;
mod engine;
mod host_memory;
mod instance;
mod module;

pub use buffer::JitBuffer;
pub use builtins::BuiltinTable;
pub use compiler::compile_module_jit;
pub use engine::NativeJitEngine;
pub use instance::NativeJitInstance;
pub use module::{NativeJitDirectCall, NativeJitModule};
