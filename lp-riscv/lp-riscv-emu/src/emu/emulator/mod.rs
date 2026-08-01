//! RISC-V 32-bit emulator implementation.
//!
//! This module contains the emulator implementation broken down into logical submodules:
//! - `state`: Core state and initialization
//! - `registers`: Register and PC management
//! - `execution`: Instruction execution
//! - `function_call`: Function calling with ABI setup
//! - `run_loops`: High-level run methods
//! - `debug`: Debug formatting and logging
//!
//! The public result types (`StepResult`, `SyscallInfo`, `PanicInfo`,
//! `OomInfo`) live in `lp_emu_core` (arch-neutral run-loop contract) —
//! import them from there.

mod backtrace;
mod debug;
mod execution;
mod function_call;
mod registers;
mod run_loops;
mod state;

#[cfg(feature = "std")]
pub use state::FrameOutcome;
pub use state::{DEFAULT_CALL_INSTRUCTION_LIMIT, Riscv32Emulator};
