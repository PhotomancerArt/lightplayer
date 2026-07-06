//! Logging infrastructure for fw-core.
//!
//! Provides the emulator logger (routes to syscalls). The ESP32 target owns
//! its own logger in `fw-esp32/src/logger.rs`; the retired `fw-core` ESP32
//! logger duplicate was removed with ADR 2026-07-05-studio-logging-model.

#[cfg(feature = "emu")]
pub mod emu;

// Re-export initialization functions
#[cfg(feature = "emu")]
pub use emu::{init as init_emu_logger, lp_jit_host_log};
