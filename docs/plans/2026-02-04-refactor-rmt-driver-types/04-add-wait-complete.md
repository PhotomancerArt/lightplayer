# Phase 4: Add LedTransaction::wait_complete() and update test to use full new API

## Scope of phase

Add `wait_complete()` method to `LedTransaction` that waits for transmission to complete and returns the `LedChannel`. Update the test to use the complete new API (`channel.start_transmission().wait_complete()`), exercising the full transaction pattern.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add wait_complete method to LedTransaction

Add the method that polls `ChannelState` and returns `LedChannel`:

```rust
impl<'ch> LedTransaction<'ch> {
    /// Wait for transmission to complete
    ///
    /// # Returns
    /// The `LedChannel` instance, ready for the next transmission
    pub fn wait_complete(self) -> LedChannel<'ch> {
        let channel_idx = self.channel.channel_idx as usize;
        
        // Poll ChannelState until frame is complete
        while !CHANNEL_STATE[channel_idx]
            .frame_complete
            .load(Ordering::Acquire)
        {
            // Small delay to avoid busy waiting
            esp_hal::delay::Delay::new().delay_micros(10);
        }

        // Return the channel for reuse
        self.channel
    }
}
```

### 2. Update test_rmt.rs to use full new API

Update all test patterns to use the complete new API:

```rust
// Replace:
// rmt_ws2811_write_bytes(&data);
// rmt_ws2811_wait_complete();

// With:
let tx = channel.start_transmission(&data);
channel = tx.wait_complete();
```

Update the loop to reuse the channel:

```rust
let mut channel = LedChannel::new(&mut rmt, pin, NUM_LEDS)
    .expect("Failed to initialize LED channel");

loop {
    // Test 1: Solid red
    println!("Test: Solid red");
    let mut data = [0u8; NUM_LEDS * 3];
    // ... fill data ...
    let tx = channel.start_transmission(&data);
    channel = tx.wait_complete();

    // Test 2: Solid green
    // ... etc ...
}
```

### 3. Keep old API wrappers working

Ensure `rmt_ws2811_wait_complete()` still works for backward compatibility (it can use `CHANNEL_STATE[RMT_CH_IDX]` directly).

## Tests

Update `test_rmt.rs` to use the complete new API: `channel.start_transmission().wait_complete()`.

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
- Full new API works: `channel.start_transmission().wait_complete()`
- Channel is properly returned and can be reused
- LEDs update correctly with all test patterns
- No regressions in behavior
- Serial output confirms full transaction pattern is working
