# Design: Refactor RMT Driver to Use LedChannel and LedTransaction Types

## Scope of Work

Refactor the RMT driver to use proper ownership types (`LedChannel` and `LedTransaction`) instead of leaking channels and using multiple global statics. The goal is a clean API similar to esp-hal's transaction pattern, with only one clean global (`ChannelState`) for interrupt handler coordination.

## File Structure

```
lp-fw/fw-esp32/src/output/
├── mod.rs                    # UPDATE: Export LedChannel, LedTransaction, remove old init functions
├── provider.rs               # (unchanged)
└── rmt_driver.rs             # REFACTOR: Add LedChannel, LedTransaction, ChannelState
    ├── ChannelState struct   # NEW: Atomic global state for interrupt handler
    ├── LedChannel struct     # NEW: Owns Channel, manages LED buffer and channel lifecycle
    ├── LedTransaction struct # NEW: Represents in-progress transmission
    ├── rmt_interrupt_handler # UPDATE: Access ChannelState instead of individual globals
    ├── start_transmission    # UPDATE: Take channel_idx parameter, use ChannelState
    └── Legacy wrappers       # TEMPORARY: rmt_ws2811_write_bytes, rmt_ws2811_wait_complete
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Global State                             │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ ChannelState[2] (atomic)                             │  │
│  │   - frame_complete: AtomicBool                       │  │
│  │   - led_counter: AtomicUsize                         │  │
│  │   - frame_counter: AtomicUsize                       │  │
│  │   - stats_count: AtomicI32                           │  │
│  │   - stats_sum: AtomicI32                             │  │
│  └─────────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Interrupt Handler (global, set once)                 │  │
│  │   - Reads ChannelState[channel_idx]                  │  │
│  │   - Updates frame_complete, counters, stats         │  │
│  └─────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            ▲
                            │ accesses
                            │
┌─────────────────────────────────────────────────────────────┐
│                    LedChannel                                │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ - channel: Channel<'ch, Blocking, Tx>               │   │
│  │ - channel_idx: u8 (0 for now, const RMT_CH_IDX)    │   │
│  │ - num_leds: usize                                   │   │
│  │ - led_buffer: Box<[RGB8]>                          │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  new(rmt, pin, num_leds) -> Result<Self>                    │
│    - Sets up interrupt handler (if first channel)          │
│    - Configures channel                                     │
│    - Initializes RMT memory                                 │
│    - Enables interrupts                                     │
│                                                              │
│  start_transmission(self, rgb_bytes) -> LedTransaction     │
│    - Writes data to led_buffer                              │
│    - Calls start_transmission() with channel_idx            │
│    - Returns LedTransaction                                 │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ consumes
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                 LedTransaction                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ - channel: LedChannel                                │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  wait_complete(self) -> LedChannel                          │
│    - Polls ChannelState[channel_idx].frame_complete         │
│    - Returns LedChannel when complete                        │
└─────────────────────────────────────────────────────────────┘
```

## Main Components

### ChannelState
- **Purpose**: Single atomic global struct containing all per-channel state needed by interrupt handler
- **Fields**: All atomic types for thread-safe access
- **Access**: Indexed by `channel_idx` (0 or 1), currently only [0] used
- **Location**: Global static in `rmt_driver.rs`

### LedChannel
- **Purpose**: Owns and manages RMT channel lifecycle
- **Owns**: `Channel<'ch, Blocking, Tx>`, LED buffer, channel configuration
- **Methods**:
  - `new()`: Creates channel, sets up interrupt handler (if first), configures hardware
  - `start_transmission()`: Consumes self, writes data, starts transmission, returns `LedTransaction`

### LedTransaction
- **Purpose**: Represents an in-progress transmission
- **Owns**: `LedChannel` (prevents channel from being dropped)
- **Methods**:
  - `wait_complete()`: Consumes self, polls `ChannelState`, returns `LedChannel` when done
- **Attribute**: `#[must_use]` to prevent accidental drops

### Interrupt Handler
- **Purpose**: Handles RMT interrupts, updates `ChannelState`
- **Setup**: Called once globally (first `LedChannel::new()` or separate init)
- **Access**: Reads/writes `ChannelState[channel_idx]` based on which channel triggered interrupt

## Interaction Flow

1. **Initialization**:
   ```
   let channel = LedChannel::new(rmt, pin, num_leds)?;
   // Sets up interrupt handler (if first)
   // Configures channel
   // Initializes ChannelState[channel_idx]
   ```

2. **Start Transmission**:
   ```
   let tx = channel.start_transmission(rgb_bytes);
   // Writes data to led_buffer
   // Calls start_transmission() which:
   //   - Updates ChannelState[channel_idx].frame_complete = false
   //   - Writes to RMT memory
   //   - Starts hardware transmission
   ```

3. **Interrupt Handler** (runs asynchronously):
   ```
   rmt_interrupt_handler()
   // Reads ChannelState[channel_idx]
   // Updates frame_complete, counters, stats
   ```

4. **Wait for Completion**:
   ```
   let channel = tx.wait_complete();
   // Polls ChannelState[channel_idx].frame_complete
   // Returns LedChannel when complete
   ```

5. **Repeat**: Use `channel` to start next transmission

## Migration Strategy

Each phase exercises the new code immediately, ensuring it works before moving on:

- **Phase 1**: Add `ChannelState` struct, migrate interrupt handler to use it
  - Old functions still work, just accessing state differently
  - Test: Run existing test, verify interrupts still work (new code exercised via interrupt handler)
  
- **Phase 2**: Add `LedChannel` type, update test to create and store it
  - Add `LedChannel::new()` that sets up interrupt handler and configures channel
  - Update `test_rmt.rs` to call `LedChannel::new()` and store the channel
  - Keep using old `rmt_ws2811_write_bytes` and `rmt_ws2811_wait_complete` wrappers
  - Test: Verify `LedChannel::new()` works, channel is properly configured
  
- **Phase 3**: Add `LedChannel::start_transmission()` method, update test to use it
  - Add method that writes data and calls `start_transmission()`
  - Update `test_rmt.rs` to call `channel.start_transmission()` instead of `rmt_ws2811_write_bytes()`
  - Keep using old `rmt_ws2811_wait_complete` wrapper
  - Test: Verify `start_transmission()` works, LEDs update correctly
  
- **Phase 4**: Add `LedTransaction` type and `wait_complete()` method, update test to use it
  - `start_transmission()` now returns `LedTransaction`
  - Add `LedTransaction::wait_complete()` that returns `LedChannel`
  - Update `test_rmt.rs` to use full new API: `channel.start_transmission().wait_complete()`
  - Test: Verify full new API works end-to-end
  
- **Phase 5**: Remove legacy wrapper functions and old init functions
  - Remove `rmt_ws2811_init`, `rmt_ws2811_init2`, `rmt_ws2811_write_bytes`, `rmt_ws2811_wait_complete`
  - Update `mod.rs` exports
  - Test: Verify everything still compiles and works with new API only
  
- **Phase 6**: Cleanup and validation
  - Remove debug prints, fix warnings, final validation
  - Test: Full validation
