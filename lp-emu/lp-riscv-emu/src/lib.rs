//! RISC-V 32-bit emulator runtime.
//!
//! This crate provides a complete RISC-V 32-bit emulator for testing and debugging
//! generated code. It includes:
//! - Full RISC-V 32-bit instruction set emulation
//! - Serial communication support for I/O
//! - Memory management and protection
//! - Step-by-step execution with logging and debugging capabilities

#![no_std]

// With `std` enabled, `alloc` resolves via the extern prelude and a root
// `extern crate alloc` would trip `unused_extern_crates`; without `std` it
// is required for the crate's `alloc::` imports.
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

// Re-export instruction utilities for convenience
pub use lp_riscv_inst::{Gpr, Inst, decode_instruction, format_instruction};

// Emulator modules
pub mod emu;

#[cfg(feature = "std")]
pub mod test_util;

// Re-exports for convenience
#[cfg(feature = "std")]
pub use emu::FrameOutcome;
pub use emu::{
    DEFAULT_CALL_INSTRUCTION_LIMIT, EmulatorError, InstLog, Riscv32Emulator,
    trap_code_from_cranelift,
};

#[cfg(feature = "std")]
pub use test_util::{BinaryBuildConfig, ensure_binary_built, find_workspace_root};

/// Initialize logging for emulator host
///
/// Should be called before running guest code.
/// Reads RUST_LOG environment variable for filtering.
#[cfg(feature = "std")]
pub fn init_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}
