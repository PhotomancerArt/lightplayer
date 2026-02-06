# Phase 7: Add Test Features

## Scope of phase

Add test features (e.g., `test_rmt`) that bypass the LightPlayer engine and run the RMT driver in test mode with simple patterns.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Cargo.toml

Add test feature:

```toml
[features]
default = ["esp32c6"]
esp32c6 = [
    "esp-backtrace/esp32c6",
    "esp-bootloader-esp-idf/esp32c6",
    "esp-rtos/esp32c6",
    "esp-hal/esp32c6",
]
test_rmt = []  # Test RMT driver with simple patterns
```

### 2. Create tests/test_rmt.rs

Create test mode that runs RMT driver with simple patterns:

```rust
//! RMT driver test mode
//!
//! When `test_rmt` feature is enabled, this runs simple LED patterns
//! to verify the RMT driver works correctly.

use esp_hal::gpio::Output;
use esp_hal::peripherals::Peripherals;
use esp_hal::rmt::Rmt;
use esp_println::println;
use smart_leds::RGB8;

use crate::output::rmt_driver::{rmt_ws2811_init, rmt_ws2811_write_bytes, rmt_ws2811_wait_complete};

/// Run RMT test mode
///
/// Displays simple patterns on LEDs to verify RMT driver works.
pub async fn run_rmt_test() -> ! {
    println!("RMT test mode starting...");

    let peripherals = Peripherals::take();
    
    // Configure RMT
    let rmt: Rmt<'_, esp_hal::Blocking> = {
        let frequency: Rate = Rate::from_mhz(80);
        Rmt::new(peripherals.RMT, frequency)
    }
    .expect("Failed to initialize RMT");
    
    // Use GPIO8 for LED output (adjust as needed)
    let pin = peripherals.GPIO10.into_output();
    
    // Initialize RMT driver for 8 LEDs
    const NUM_LEDS: usize = 64;
    let _transaction = rmt_ws2811_init(rmt, pin, NUM_LEDS)
        .expect("Failed to initialize RMT driver");

    println!("RMT driver initialized, starting test patterns...");

    loop {
        // Test 1: Solid red
        println!("Test: Solid red");
        let mut data = [0u8; NUM_LEDS * 3];
        for i in 0..NUM_LEDS {
            data[i * 3] = 255;     // R
            data[i * 3 + 1] = 0;   // G
            data[i * 3 + 2] = 0;   // B
        }
        rmt_ws2811_write_bytes(&data);
        rmt_ws2811_wait_complete();
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;

        // Test 2: Solid green
        println!("Test: Solid green");
        for i in 0..NUM_LEDS {
            data[i * 3] = 0;       // R
            data[i * 3 + 1] = 255; // G
            data[i * 3 + 2] = 0;   // B
        }
        rmt_ws2811_write_bytes(&data);
        rmt_ws2811_wait_complete();
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;

        // Test 3: Solid blue
        println!("Test: Solid blue");
        for i in 0..NUM_LEDS {
            data[i * 3] = 0;       // R
            data[i * 3 + 1] = 0;   // G
            data[i * 3 + 2] = 255; // B
        }
        rmt_ws2811_write_bytes(&data);
        rmt_ws2811_wait_complete();
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;

        // Test 4: Rainbow pattern
        println!("Test: Rainbow pattern");
        for i in 0..NUM_LEDS {
            let hue = (i * 360 / NUM_LEDS) as f32;
            let rgb = hsv_to_rgb(hue, 1.0, 1.0);
            data[i * 3] = rgb.0;
            data[i * 3 + 1] = rgb.1;
            data[i * 3 + 2] = rgb.2;
        }
        rmt_ws2811_write_bytes(&data);
        rmt_ws2811_wait_complete();
        embassy_time::Timer::after(embassy_time::Duration::from_secs(2)).await;

        // Test 5: Chase pattern
        println!("Test: Chase pattern");
        for offset in 0..NUM_LEDS {
            for i in 0..NUM_LEDS {
                if i == offset {
                    data[i * 3] = 255;
                    data[i * 3 + 1] = 255;
                    data[i * 3 + 2] = 255;
                } else {
                    data[i * 3] = 0;
                    data[i * 3 + 1] = 0;
                    data[i * 3 + 2] = 0;
                }
            }
            rmt_ws2811_write_bytes(&data);
            rmt_ws2811_wait_complete();
            embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        }
    }
}

/// Convert HSV to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
```

### 3. Update main.rs

Add test mode support:

```rust
#[cfg(feature = "test_rmt")]
mod tests {
    pub mod test_rmt;
}

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    #[cfg(feature = "test_rmt")]
    {
        use tests::test_rmt::run_rmt_test;
        run_rmt_test().await;
    }

    #[cfg(not(feature = "test_rmt"))]
    {
        // Normal server loop code from Phase 6
        // ... existing code ...
    }
}
```

## Notes

- Test mode bypasses the full server stack
- User can visually verify LED output
- Patterns include: solid colors, rainbow, chase
- GPIO pin (GPIO8) may need adjustment based on hardware
- Number of LEDs (8) may need adjustment

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6,test_rmt
```

Expected: Code compiles. Test mode can be run with `cargo run --features test_rmt` (when hardware is connected).
