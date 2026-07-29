//! Architecture-neutral emulator infrastructure.
//!
//! Extracted from `lp-riscv-emu` so emulators for other architectures can
//! share the same host-side machinery. It includes:
//! - Serial communication support for guest I/O
//! - Logging verbosity levels
//! - Per-instruction cycle-cost accounting
//! - Emulator time control (real vs simulated)
//! - The guest memory model ([`Memory`])
//! - The run-loop result contract ([`StepResult`]) and trap codes ([`TrapCode`])
//! - Host-side profiling (collectors, sessions, trace layout) behind the `std`
//!   feature; arch-specific bits (stack unwinding, RAM start) are injected

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

// Compile-time configuration
pub mod config;

pub mod cycle_model;
pub mod log_level;
pub mod memory;
#[cfg(feature = "std")]
pub mod profile;
pub mod serial;
pub mod step_result;
pub mod time;
pub mod trap_code;

// Re-exports for convenience
pub use cycle_model::{CycleModel, InstClass};
pub use log_level::LogLevel;
pub use memory::{DEFAULT_RAM_START, DEFAULT_SHARED_START, Memory, MemoryAccessKind, MemoryError};
pub use step_result::{OomInfo, PanicInfo, StepResult, SyscallInfo};
pub use time::TimeMode;
pub use trap_code::{TrapCode, trap_code_to_string};
