# Phase 2: Add LedChannel type and update test to use it

## Scope of phase

Create the `LedChannel` struct that owns the RMT channel and LED buffer. Update the test code to create and store a `LedChannel` instance, exercising the new type immediately.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add LedChannel struct

Add the `LedChannel` struct after the constants and before the helper functions:

```rust
/// LED channel for WS2811/WS2812 LEDs using RMT
pub struct LedChannel<'ch> {
    channel: Channel<'ch, Blocking, Tx>,
    channel_idx: u8,
    num_leds: usize,
    led_buffer: Box<[RGB8]>,
}

impl<'ch> LedChannel<'ch> {
    /// Create a new LED channel
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral (mutable reference, will set interrupt handler if first channel)
    /// * `pin` - GPIO pin for LED data output
    /// * `num_leds` - Number of LEDs in the strip
    ///
    /// # Returns
    /// `LedChannel` instance that owns the RMT channel
    pub fn new<O>(
        rmt: &mut Rmt<'ch, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<Self, RmtError>
    where
        O: PeripheralOutput<'ch>,
    {
        extern crate alloc;
        use alloc::boxed::Box;
        use alloc::vec;

        // Set up interrupt handler (only needs to be done once, but safe to call multiple times)
        // TODO: Use a static flag to only set up once
        let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
        rmt.set_interrupt_handler(handler);

        // Configure the RMT channel
        let config = create_rmt_config();
        let channel = rmt.channel0.configure_tx(pin, config)?;

        // Allocate LED buffer
        let led_buffer = vec![RGB8 { r: 0, g: 0, b: 0 }; num_leds].into_boxed_slice();

        // Initialize RMT memory with zeros
        let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
        unsafe {
            for j in 0..BUFFER_SIZE {
                rmt_base.add(j).write_volatile(0);
            }
        }

        // Enable interrupts
        let rmt_regs = esp_hal::peripherals::RMT::regs();
        rmt_regs.int_ena().modify(|_, w| {
            w.ch_tx_thr_event(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_end(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_err(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_loop(RMT_CH_IDX as u8).clear_bit()
        });

        // Set initial threshold configuration
        rmt_regs.ch_tx_lim(RMT_CH_IDX).modify(|_, w| {
            w.loop_count_reset().set_bit();
            w.tx_loop_cnt_en().set_bit();
            w.tx_loop_num().bits(0);
            w.tx_lim().bits(HALF_BUFFER_SIZE as u16)
        });

        // Configure initial channel settings (single-shot, wrap enabled)
        rmt_regs.ch_tx_conf0(RMT_CH_IDX).modify(|_, w| {
            w.tx_conti_mode().clear_bit(); // single-shot
            w.mem_tx_wrap_en().set_bit() // wrap enabled
        });

        // Update configuration
        rmt_regs.ch_tx_conf0(RMT_CH_IDX).modify(|_, w| w.conf_update().set_bit());

        Ok(Self {
            channel,
            channel_idx: RMT_CH_IDX as u8,
            num_leds,
            led_buffer,
        })
    }
}
```

### 2. Update rmt_ws2811_init2 to use LedChannel internally

Modify `rmt_ws2811_init2` to create a `LedChannel` and store it in a static (for backward compatibility):

```rust
// Static to store channel for backward compatibility with old API
static mut LED_CHANNEL_STORAGE: Option<LedChannel<'static>> = None;

pub fn rmt_ws2811_init2<'d, O>(
    mut rmt: esp_hal::rmt::Rmt<'d, Blocking>,
    pin: O,
    num_leds: usize,
) -> Result<(), RmtError>
where
    O: PeripheralOutput<'d>,
{
    // Update globals for old API compatibility
    unsafe {
        ACTUAL_NUM_LEDS = num_leds;
        // Note: LedChannel owns the buffer, but we need to expose it for old API
        // TODO: Remove this when old API is removed
    }

    // Create LedChannel
    let channel = LedChannel::new(&mut rmt, pin, num_leds)?;
    
    // Store channel in static to keep it alive
    // TODO: Remove this when old API is removed
    unsafe {
        LED_CHANNEL_STORAGE = Some(channel);
        // Also update LED_DATA_BUFFER_PTR for old API
        if let Some(ref ch) = LED_CHANNEL_STORAGE {
            LED_DATA_BUFFER_PTR = ch.led_buffer.as_ptr() as *mut RGB8;
        }
    }

    Ok(())
}
```

### 3. Update test_rmt.rs to create LedChannel

Update `test_rmt.rs` to create and store a `LedChannel`:

```rust
use crate::output::LedChannel;  // Add import

// In run_rmt_test():
// Replace:
// rmt_ws2811_init2(rmt, pin, NUM_LEDS).expect("Failed to initialize RMT driver");

// With:
let mut channel = LedChannel::new(&mut rmt, pin, NUM_LEDS)
    .expect("Failed to initialize LED channel");

// Store channel in a variable (we'll use it in next phase)
// For now, still use old API for write/wait
```

### 4. Keep old API working

Ensure `rmt_ws2811_write_bytes` and `rmt_ws2811_wait_complete` still work by accessing the stored channel's buffer via globals (temporary).

## Tests

Update `test_rmt.rs` to create `LedChannel` and verify it works. The test should still use old `rmt_ws2811_write_bytes` and `rmt_ws2811_wait_complete` for now.

## Validate

Build and test:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo build --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Then flash and test manually:
```bash
cargo run --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Verify:
- `LedChannel::new()` successfully creates a channel
- Channel is properly configured (LEDs still work via old API)
- No regressions in behavior
- Serial output shows channel was created successfully
