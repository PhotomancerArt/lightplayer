//! RISC-V 32-bit emulator runtime.
//!
//! This crate provides a complete RISC-V 32-bit emulator for testing and debugging
//! generated code. It includes:
//! - Full RISC-V 32-bit instruction set emulation
//! - Serial communication support for I/O
//! - Memory management and protection
//! - Step-by-step execution with logging and debugging capabilities

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

// Re-export instruction utilities for convenience
pub use lp_riscv_inst::{Gpr, Inst, decode_instruction, format_instruction};

// Re-export debug macro - since lp-riscv-inst uses #[macro_export],
// we need to re-export it here to make it available
#[cfg(feature = "std")]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        lp_riscv_inst::debug!($($arg)*);
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        // No-op in no_std mode
    };
}

// Emulator modules
pub mod emu;
pub mod serial;
pub mod time;

#[cfg(feature = "std")]
pub mod test_util;

// Re-exports for convenience
pub use emu::{
    EmulatorError, InstLog, LogLevel, MemoryAccessKind, PanicInfo, Riscv32Emulator, StepResult,
    SyscallInfo, trap_code_to_string,
};
pub use time::TimeMode;

#[cfg(feature = "std")]
pub use test_util::{BinaryBuildConfig, ensure_binary_built, find_workspace_root};
