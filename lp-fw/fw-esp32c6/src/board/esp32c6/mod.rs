//! ESP32-C6 specific board initialization
//!
//! This module contains board-specific code for ESP32-C6.
//! To add support for another board (e.g., ESP32-C3), create a similar file
//! and add feature gates in board/mod.rs.

pub mod constants;
#[cfg(any(
    feature = "test_msafluid",
    feature = "test_jit_math_perf",
    feature = "test_shader_compile_incremental",
))]
pub mod cycle_counter;
pub mod init;
// Sole consumer is `serial::io_task`; keep this gate identical to its own.
#[cfg(any(not(fw_harness), feature = "test_json"))]
pub mod usb_connection;
