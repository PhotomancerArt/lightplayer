//! ESP32-S3 chip constants.

/// CPU clock after `esp_hal::init` with the default config, in Hz.
///
/// The S3 runs at 240 MHz — notably faster than the C6's 160 MHz, which is why
/// cycle counts are not comparable between the two chips without converting to
/// time first.
pub const CPU_HZ: u64 = 240_000_000;
