# Phase 11: Implement ESP32 LED output (RMT driver integration)

## Goal

Implement LED output on ESP32 using custom RMT driver.

## Tasks

1. Copy RMT driver code from reference (`lpmini2024/apps/fw-esp32c3/src/rmt_ws2811_driver.rs`)
2. Adapt RMT driver for ESP32-C6 (if needed)
3. Create `src/led_output.rs`:
   - Implement `lp-core::traits::LedOutput` trait for ESP32
   - Wrap RMT driver with trait implementation
   - Handle pixel data conversion (RGB/RGBA to WS2812 format)
4. Initialize RMT/LED output in `main.rs`:
   - Set up RMT peripheral
   - Create LED output instance
5. Test basic LED output (simple pattern)

## Success Criteria

- LED output trait implementation works
- Can write pixels to WS2812 LEDs
- RMT driver works on ESP32-C6
- All code compiles without warnings

