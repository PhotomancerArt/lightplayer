# Phase 5: Remove legacy wrapper functions and old init functions

## Scope of phase

Remove the old API functions (`rmt_ws2811_init`, `rmt_ws2811_init2`, `rmt_ws2811_write_bytes`, `rmt_ws2811_wait_complete`) and clean up the static storage. Update module exports to only expose the new API.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Remove old init functions

Remove these functions:
- `rmt_ws2811_init()` - old function that returned transaction handle
- `rmt_ws2811_init2()` - old function that leaked channel

### 2. Remove wrapper functions

Remove these wrapper functions:
- `rmt_ws2811_write_bytes()` - wrapper that used globals
- `rmt_ws2811_wait_complete()` - wrapper that polled globals

### 3. Remove static storage

Remove the temporary static that stored the channel:
```rust
// Remove this:
static mut LED_CHANNEL_STORAGE: Option<LedChannel<'static>> = None;
```

### 4. Remove old global statics

Remove these globals (they're now in `LedChannel`):
- `ACTUAL_NUM_LEDS: usize`
- `LED_DATA_BUFFER_PTR: *mut RGB8`

### 5. Update start_transmission_with_state

Since we're removing the globals, we need to update `start_transmission_with_state` to not rely on them. The interrupt handler still needs access to the buffer, so we need a different approach.

Options:
- Store buffer pointer in `ChannelState` (add field)
- Pass buffer info through `ChannelState` when starting transmission

Let's add buffer info to `ChannelState`:

```rust
struct ChannelState {
    frame_complete: AtomicBool,
    led_counter: AtomicUsize,
    frame_counter: AtomicUsize,
    stats_count: AtomicI32,
    stats_sum: AtomicI32,
    // Add buffer info for interrupt handler
    led_buffer_ptr: AtomicPtr<RGB8>,  // Pointer to current buffer
    num_leds: AtomicUsize,             // Number of LEDs
}

impl ChannelState {
    const fn new() -> Self {
        Self {
            frame_complete: AtomicBool::new(true),
            led_counter: AtomicUsize::new(0),
            frame_counter: AtomicUsize::new(0),
            stats_count: AtomicI32::new(0),
            stats_sum: AtomicI32::new(0),
            led_buffer_ptr: AtomicPtr::new(core::ptr::null_mut()),
            num_leds: AtomicUsize::new(0),
        }
    }
}
```

Update `start_transmission_with_state` to set buffer info in `ChannelState`:

```rust
unsafe fn start_transmission_with_state(
    channel_idx: u8,
    led_buffer_ptr: *mut RGB8,
    num_leds: usize,
) {
    let ch_idx = channel_idx as usize;
    
    // Store buffer info in ChannelState for interrupt handler
    CHANNEL_STATE[ch_idx].led_buffer_ptr.store(led_buffer_ptr, Ordering::Release);
    CHANNEL_STATE[ch_idx].num_leds.store(num_leds, Ordering::Release);
    
    // ... rest of start_transmission logic ...
}
```

Update `write_half_buffer` to read buffer info from `ChannelState`:

```rust
unsafe fn write_half_buffer(is_first_half: bool, channel_idx: u8) -> bool {
    let ch_idx = channel_idx as usize;
    let num_leds = CHANNEL_STATE[ch_idx].num_leds.load(Ordering::Acquire);
    let buffer_ptr = CHANNEL_STATE[ch_idx].led_buffer_ptr.load(Ordering::Acquire);
    
    // ... use buffer_ptr and num_leds instead of globals ...
}
```

### 6. Update module exports

Update `mod.rs` to only export new API:

```rust
pub use rmt_driver::{LedChannel, LedTransaction};
```

Remove exports of old functions.

### 7. Update LedChannel::start_transmission

Update to set buffer info in `ChannelState`:

```rust
pub fn start_transmission(mut self, rgb_bytes: &[u8]) -> LedTransaction<'ch> {
    // ... write data to buffer ...
    
    // Store buffer info in ChannelState for interrupt handler
    CHANNEL_STATE[self.channel_idx as usize]
        .led_buffer_ptr
        .store(self.led_buffer.as_ptr() as *mut RGB8, Ordering::Release);
    CHANNEL_STATE[self.channel_idx as usize]
        .num_leds
        .store(self.num_leds, Ordering::Release);
    
    // Start transmission
    unsafe {
        start_transmission_with_state(
            self.channel_idx,
            self.led_buffer.as_ptr() as *mut RGB8,
            self.num_leds,
        );
    }
    
    LedTransaction { channel: self }
}
```

## Tests

Test code should already be using the new API from previous phases. Verify it still works.

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
- Everything compiles without old functions
- Test still works with new API only
- LEDs update correctly
- No regressions in behavior
- Check that old function names are no longer exported
