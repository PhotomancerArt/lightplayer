//! RISC-V 32-bit emulator runtime.
//!
//! This crate provides:
//! - RISC-V 32-bit emulator for testing generated code
//! - Serial communication support
//! - Memory management
//! - Instruction execution

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

// Re-exports for convenience
pub use emu::{
    EmulatorError, InstLog, LogLevel, MemoryAccessKind, PanicInfo, Riscv32Emulator, StepResult,
    SyscallInfo, trap_code_to_string,
};
