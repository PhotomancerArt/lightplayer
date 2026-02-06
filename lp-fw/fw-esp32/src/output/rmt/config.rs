//! RMT driver configuration constants and timing parameters
//!
//! This module contains all configuration constants, timing parameters, and pulse code
//! generation for WS2811/WS2812 LEDs.

use esp_hal::gpio::Level;
use esp_hal::rmt::TxChannelConfig;

// Buffer size for 8 LEDs worth of data (double buffered)
// Using memsize(4) = 192 words. 8 LEDs = 192 words exactly, 4 LEDs per half
pub(crate) const BUFFER_LEDS: usize = 8;
pub(crate) const HALF_BUFFER_LEDS: usize = BUFFER_LEDS / 2;
pub(crate) const BITS_PER_LED: usize = 3 * 8;
pub(crate) const HALF_BUFFER_SIZE: usize = (BUFFER_LEDS * BITS_PER_LED) / 2;
pub(crate) const BUFFER_SIZE: usize = BUFFER_LEDS * BITS_PER_LED;

// Channel index constant (easy to find and change later for multi-channel support)
pub(crate) const RMT_CH_IDX: usize = 0;

// Source clock frequency in MHz
const SRC_CLOCK_MHZ: u32 = 80;

// LED timing constants for WS2812/SK6812
const SK68XX_CODE_PERIOD: u32 = 1250; // 800kHz
const SK68XX_T0H_NS: u32 = 400;
const SK68XX_T0L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T0H_NS;
const SK68XX_T1H_NS: u32 = 850;
const SK68XX_T1L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T1H_NS;
const SK68XX_LATCH_NS: u32 = 50_000;

/// Pulse code for a zero bit (WS2812)
pub(crate) const PULSE_ZERO: u32 = pulse_code(
    Level::High,
    ((SK68XX_T0H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_T0L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
);

/// Pulse code for a one bit (WS2812)
pub(crate) const PULSE_ONE: u32 = pulse_code(
    Level::High,
    ((SK68XX_T1H_NS * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_T1L_NS * SRC_CLOCK_MHZ) / 1000) as u16,
);

/// Pulse code for latch (WS2812 requires 50us+ low to latch)
/// At 80MHz: 50us = 4000 ticks, split as 2000+2000
pub(crate) const PULSE_LATCH: u32 = pulse_code(
    Level::Low,
    ((SK68XX_LATCH_NS / 2 * SRC_CLOCK_MHZ) / 1000) as u16,
    Level::Low,
    ((SK68XX_LATCH_NS / 2 * SRC_CLOCK_MHZ) / 1000) as u16,
);

/// Create a pulse code from level and length pairs
const fn pulse_code(level1: Level, length1: u16, level2: Level, length2: u16) -> u32 {
    let level1 = (level_bit(level1)) | (length1 as u32 & 0b111_1111_1111_1111);
    let level2 = (level_bit(level2)) | (length2 as u32 & 0b111_1111_1111_1111);
    level1 | (level2 << 16)
}

/// Convert GPIO level to bit value for pulse code
const fn level_bit(level: Level) -> u32 {
    match level {
        Level::Low => 0u32,
        Level::High => 1u32 << 15,
    }
}

/// Create RMT channel configuration for WS2811/WS2812 LEDs
// Used internally by LedChannel::new
#[allow(dead_code, reason = "used internally by LedChannel")]
pub(crate) fn create_rmt_config() -> TxChannelConfig {
    TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_idle_output(true)
        .with_memsize(4) // Use all 4 memory blocks - 192 words
}
