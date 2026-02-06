//! Logging infrastructure for fw-core.
//!
//! Provides logger implementations for different environments:
//! - Emulator: Routes to syscalls
//! - ESP32: Routes to a provided print function (typically USB serial)

#[cfg(feature = "emu")]
pub mod emu;

#[cfg(feature = "esp32")]
pub mod esp32;

// Re-export initialization functions
#[cfg(feature = "emu")]
pub use emu::{init as init_emu_logger, lp_jit_host_log};

#[cfg(feature = "esp32")]
pub use esp32::{PrintFn, init as init_esp32_logger};
