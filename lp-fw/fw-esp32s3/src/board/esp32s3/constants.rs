//! ESP32-S3 chip constants.

/// Highest usable GPIO number on the ESP32-S3 (GPIO0..=GPIO48).
///
/// Gated like its only consumer, `crate::hardware` (the GPIO button driver):
/// harness builds that compile neither would otherwise fail `-D dead_code`.
#[cfg(any(not(fw_harness), feature = "test_button"))]
pub const MAX_GPIO: u8 = 48;

/// CPU clock as explicitly configured by every entrypoint, in Hz.
///
/// **Not** the default-config clock: esp-hal's `CpuClock::default()` for this
/// chip is 80 MHz, and 240 MHz is `CpuClock::max()`. Both entrypoints opt in —
/// `super::init::init_board` on the app path and the `fw_harness` `boot()` in
/// `main.rs` — so this constant is only true because they do. An entrypoint
/// that takes `Config::default()` would silently make every `cycles_to_us`
/// figure understate real elapsed time by 3×.
///
/// The S3 runs at 240 MHz — notably faster than the C6's 160 MHz, which is why
/// cycle counts are not comparable between the two chips without converting to
/// time first.
pub const CPU_HZ: u64 = 240_000_000;
