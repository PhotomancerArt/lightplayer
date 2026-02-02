//! Logging infrastructure for fw-core.
//!
//! Provides logger implementations for different environments:
//! - Emulator: Routes to syscalls
//! - ESP32: Routes to esp_println

#[cfg(feature = "emu")]
pub mod emu;

#[cfg(feature = "esp32")]
pub mod esp32;

// Re-export initialization functions
#[cfg(feature = "emu")]
pub use emu::init as init_emu_logger;

#[cfg(feature = "esp32")]
pub use esp32::{PrintFn, init as init_esp32_logger};
