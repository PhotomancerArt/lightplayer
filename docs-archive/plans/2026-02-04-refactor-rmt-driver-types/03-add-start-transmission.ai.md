# Phase 3: Add LedChannel::start_transmission() and update test to use it

## Scope of phase

Add `start_transmission()` method to `LedChannel` that writes RGB data and starts transmission. Update the test to use this new method instead of `rmt_ws2811_write_bytes()`, exercising the new API immediately.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add start_transmission method to LedChannel

Add the method that consumes `self` and returns `LedTransaction` (we'll create `LedTransaction` in next phase, for now return a placeholder or use a temporary type):

```rust
impl<'ch> LedChannel<'ch> {
    // ... existing new() method ...

    /// Start a transmission with RGB byte data
    ///
    /// # Arguments
    /// * `rgb_bytes` - Raw RGB bytes (R,G,B,R,G,B,...) must be at least num_leds * 3 bytes
    ///
    /// # Returns
    /// `LedTransaction` that must be waited on to get the channel back
    pub fn start_transmission(mut self, rgb_bytes: &[u8]) -> LedTransaction<'ch> {
        // Wait for any previous transmission to complete
        while !CHANNEL_STATE[self.channel_idx as usize]
            .frame_complete
            .load(Ordering::Acquire)
        {
            esp_hal::delay::Delay::new().delay_micros(10);
        }

        // Clear buffer
        for led in self.led_buffer.iter_mut() {
            *led = RGB8 { r: 0, g: 0, b: 0 };
        }

        // Convert from bytes to RGB8 as we copy
        let num_leds = (rgb_bytes.len() / 3).min(self.num_leds);
        for i in 0..num_leds {
            let idx = i * 3;
            self.led_buffer[i] = RGB8 {
                r: rgb_bytes[idx],
                g: rgb_bytes[idx + 1],
                b: rgb_bytes[idx + 2],
            };
        }

        // Update global buffer pointer for interrupt handler (temporary)
        unsafe {
            LED_DATA_BUFFER_PTR = self.led_buffer.as_ptr() as *mut RGB8;
            ACTUAL_NUM_LEDS = self.num_leds;
        }

        // Start transmission using existing start_transmission() function
        unsafe {
            start_transmission_with_state(
                self.channel_idx,
                self.led_buffer.as_ptr() as *mut RGB8,
                self.num_leds,
            );
        }

        LedTransaction {
            channel: self,
        }
    }
}
```

### 2. Refactor start_transmission to take parameters

Create a new internal function `start_transmission_with_state` that takes channel_idx and buffer info:

```rust
// Internal function that takes explicit parameters (for use by LedChannel)
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn start_transmission_with_state(
    channel_idx: u8,
    led_buffer_ptr: *mut RGB8,
    num_leds: usize,
) {
    // Temporarily update globals for interrupt handler
    // TODO: Remove when interrupt handler accesses LedChannel state directly
    unsafe {
        LED_DATA_BUFFER_PTR = led_buffer_ptr;
        ACTUAL_NUM_LEDS = num_leds;
    }

    let rmt = esp_hal::peripherals::RMT::regs();
    let ch_idx = channel_idx as usize;

    // ... rest of start_transmission logic, using ch_idx instead of hardcoded 0 ...
    // Use CHANNEL_STATE[ch_idx] instead of CHANNEL_STATE[RMT_CH_IDX]
}
```

Keep the old `start_transmission()` function as a wrapper for backward compatibility:

```rust
// Old function for backward compatibility
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn start_transmission() {
    unsafe {
        start_transmission_with_state(
            RMT_CH_IDX as u8,
            LED_DATA_BUFFER_PTR,
            ACTUAL_NUM_LEDS,
        );
    }
}
```

### 3. Add temporary LedTransaction struct

Add a minimal `LedTransaction` struct (we'll add `wait_complete()` in next phase):

```rust
/// Represents an in-progress LED transmission
#[must_use = "transactions must be waited on to get the channel back"]
pub struct LedTransaction<'ch> {
    channel: LedChannel<'ch>,
}

impl<'ch> LedTransaction<'ch> {
    // wait_complete() will be added in next phase
}
```

### 4. Update test_rmt.rs to use start_transmission

Update the test to use the new method:

```rust
// Replace calls to rmt_ws2811_write_bytes() with:
let _tx = channel.start_transmission(&data);
// For now, still use old wait_complete
rmt_ws2811_wait_complete();
```

### 5. Update rmt_ws2811_write_bytes wrapper

Update the wrapper to use the stored channel:

```rust
pub fn rmt_ws2811_write_bytes(rgb_bytes: &[u8]) {
    rmt_ws2811_wait_complete();

    unsafe {
        // Get channel from static storage
        if let Some(ref mut channel) = LED_CHANNEL_STORAGE.as_mut() {
            // Can't move out of static, so we need to access buffer directly
            // TODO: This is temporary - will be removed when old API is removed
            let buffer = core::slice::from_raw_parts_mut(LED_DATA_BUFFER_PTR, ACTUAL_NUM_LEDS);
            // ... write data ...
            start_transmission();
        }
    }
}
```

Actually, since we can't move out of the static, keep `rmt_ws2811_write_bytes` using the old approach for now. The test will use the new API directly.

## Tests

Update `test_rmt.rs` to call `channel.start_transmission()` instead of `rmt_ws2811_write_bytes()`. Still use `rmt_ws2811_wait_complete()` for now.

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
- `channel.start_transmission()` successfully starts transmission
- LEDs update correctly with new API
- No regressions in behavior
- Serial output confirms new method is being called
