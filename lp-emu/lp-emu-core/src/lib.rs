//! Architecture-neutral emulator infrastructure.
//!
//! Extracted from `lp-riscv-emu` so emulators for other architectures can
//! share the same host-side machinery. It includes:
//! - Serial communication support for guest I/O
//! - Logging verbosity levels
//! - Per-instruction cycle-cost accounting
//! - Emulator time control (real vs simulated)

#![no_std]

extern crate alloc;

// Compile-time configuration
pub mod config;

pub mod cycle_model;
pub mod log_level;
pub mod serial;
pub mod time;

// Re-exports for convenience
pub use cycle_model::{CycleModel, InstClass};
pub use log_level::LogLevel;
pub use time::TimeMode;
