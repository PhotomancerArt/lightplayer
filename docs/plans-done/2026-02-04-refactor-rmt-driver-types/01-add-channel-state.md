# Phase 1: Add ChannelState struct and migrate interrupt handler

## Scope of phase

Create a `ChannelState` struct containing all atomic state needed by the interrupt handler. Migrate the interrupt handler to use `ChannelState` instead of individual global statics. This consolidates all interrupt-accessible state into one clean global.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create ChannelState struct

Add a `ChannelState` struct with all atomic fields:

```rust
#[derive(Debug)]
struct ChannelState {
    frame_complete: AtomicBool,
    led_counter: AtomicUsize,
    frame_counter: AtomicUsize,
    stats_count: AtomicI32,
    stats_sum: AtomicI32,
}

impl ChannelState {
    const fn new() -> Self {
        Self {
            frame_complete: AtomicBool::new(true),
            led_counter: AtomicUsize::new(0),
            frame_counter: AtomicUsize::new(0),
            stats_count: AtomicI32::new(0),
            stats_sum: AtomicI32::new(0),
        }
    }
}
```

### 2. Create global ChannelState array

Replace individual globals with an array (designed for multi-channel, but only use index 0 for now):

```rust
// Global state for interrupt handling (one per channel, currently only [0] used)
static CHANNEL_STATE: [ChannelState; 2] = [ChannelState::new(); 2];

// Channel index constant (easy to find and change later for multi-channel)
const RMT_CH_IDX: usize = 0;
```

### 3. Update interrupt handler

Modify `rmt_interrupt_handler()` to access `CHANNEL_STATE[RMT_CH_IDX]` instead of individual globals:

```rust
// Old:
FRAME_COMPLETE.store(true, Ordering::Release);
FRAME_COUNTER += 1;
LED_COUNTER += 1;

// New:
CHANNEL_STATE[RMT_CH_IDX].frame_complete.store(true, Ordering::Release);
CHANNEL_STATE[RMT_CH_IDX].frame_counter.fetch_add(1, Ordering::Relaxed);
CHANNEL_STATE[RMT_CH_IDX].led_counter.fetch_add(1, Ordering::Relaxed);
```

### 4. Update start_transmission

Modify `start_transmission()` to use `CHANNEL_STATE[RMT_CH_IDX]`:

```rust
// Old:
FRAME_COMPLETE.store(false, Ordering::Release);
LED_COUNTER = 0;

// New:
CHANNEL_STATE[RMT_CH_IDX].frame_complete.store(false, Ordering::Release);
CHANNEL_STATE[RMT_CH_IDX].led_counter.store(0, Ordering::Relaxed);
```

### 5. Update is_frame_complete

Modify `is_frame_complete()` to use `CHANNEL_STATE[RMT_CH_IDX]`:

```rust
fn is_frame_complete() -> bool {
    CHANNEL_STATE[RMT_CH_IDX].frame_complete.load(Ordering::Acquire)
}
```

### 6. Update write_half_buffer

Modify `write_half_buffer()` to use `CHANNEL_STATE[RMT_CH_IDX]`:

```rust
// Old:
if LED_COUNTER >= ACTUAL_NUM_LEDS {
    // ...
}
let color = LED_DATA_BUFFER_PTR.add(LED_COUNTER).read_volatile();
LED_COUNTER += 1;

// New:
let led_counter = CHANNEL_STATE[RMT_CH_IDX].led_counter.load(Ordering::Acquire);
if led_counter >= ACTUAL_NUM_LEDS {
    // ...
}
let color = LED_DATA_BUFFER_PTR.add(led_counter).read_volatile();
CHANNEL_STATE[RMT_CH_IDX].led_counter.fetch_add(1, Ordering::Relaxed);
```

### 7. Remove old global statics

Remove these old globals (they're now in `ChannelState`):
- `FRAME_COUNTER: usize`
- `LED_COUNTER: usize`
- `RMT_STATS_COUNT: i32`
- `RMT_STATS_SUM: i32`
- `FRAME_COMPLETE: AtomicBool`

Keep these globals for now (will be moved to `LedChannel` in later phases):
- `ACTUAL_NUM_LEDS: usize`
- `LED_DATA_BUFFER_PTR: *mut RGB8`

## Tests

No new tests needed. Existing test should continue to work.

## Validate

Run the existing test to verify interrupts still work correctly:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo build --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Then flash and test manually:
```bash
cargo run --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Verify:
- LEDs still update correctly
- Interrupts are firing (check serial output for interrupt handler messages)
- No regressions in behavior
