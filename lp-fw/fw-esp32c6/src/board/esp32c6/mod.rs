//! ESP32-C6 specific board initialization
//!
//! This module contains board-specific code for ESP32-C6.
//! To add support for another board (e.g., ESP32-C3), create a similar file
//! and add feature gates in board/mod.rs.

// The product boot's only; harnesses drive the pins they need themselves.
#[cfg(not(fw_harness))]
pub mod board_quirks;
pub mod constants;
#[cfg(any(
    feature = "test_cycle_probe",
    feature = "test_msafluid",
    feature = "test_jit_math_perf",
    feature = "test_shader_compile_incremental",
    feature = "bench_render_loop",
    feature = "diag_secure_link",
    feature = "e03_lp_sram_probe",
))]
pub mod cycle_counter;
pub mod init;
#[cfg(feature = "e03_lp_heap")]
pub mod lp_sram_heap;
#[cfg(feature = "e03_lp_sram_probe")]
pub mod lp_sram_probe;
pub mod restart;
// The product boot's only, like the quirks.
#[cfg(not(fw_harness))]
pub mod status_led;
// Sole consumer is `serial::usb_link_task`; keep this gate identical to its own.
#[cfg(any(not(fw_harness), feature = "test_json"))]
pub mod usb_connection;
