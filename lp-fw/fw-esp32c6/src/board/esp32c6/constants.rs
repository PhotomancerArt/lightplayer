//! ESP32-C6 chip facts shared across the crate.

#![allow(
    dead_code,
    reason = "harness feature configurations compile subsets of the crate; the chip facts still belong in one place"
)]

/// Highest usable GPIO number on the ESP32-C6 (GPIO0..=GPIO30).
pub const MAX_GPIO: u8 = 30;

/// CPU clock as configured by [`super::init::init_board`] (`CpuClock::max()`).
pub const CPU_HZ: u64 = 160_000_000;
