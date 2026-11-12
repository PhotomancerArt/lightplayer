# Refactor RMT Driver to Use LedChannel and LedTransaction Types

## Scope of Work

Refactor the RMT driver (`lp-fw/fw-esp32/src/output/rmt_driver.rs`) to use proper ownership types (`LedChannel` and `LedTransaction`) instead of leaking channels and using global state. The goal is to have a clean API similar to esp-hal's transaction pattern, where:

- `LedChannel` owns the RMT channel and manages its lifecycle
- `LedTransaction` represents an in-progress transmission and consumes the channel
- After waiting for completion, the channel is returned for reuse
- Only one clean global remains (for interrupt handler state)
- Code compiles and is ready for manual testing after each phase

## Current State

The current implementation:

- Uses `rmt_ws2811_init2()` which leaks the channel using `Box::leak()` to keep it alive
- Uses multiple global statics:
  - `ACTUAL_NUM_LEDS: usize` - number of LEDs
  - `LED_DATA_BUFFER_PTR: *mut RGB8` - pointer to LED data buffer
  - `FRAME_COUNTER: usize` - counter for completed frames
  - `LED_COUNTER: usize` - current LED position in transmission
  - `RMT_STATS_COUNT: i32` - statistics counter
  - `RMT_STATS_SUM: i32` - statistics sum
  - `FRAME_COMPLETE: AtomicBool` - atomic flag for frame completion
- Has free functions: `rmt_ws2811_write_bytes()`, `rmt_ws2811_wait_complete()`
- Interrupt handler `rmt_interrupt_handler()` accesses globals directly
- Currently working correctly with atomic synchronization

## Questions

1. **Channel ownership and lifetime**: Should `LedChannel` own the `Channel<'ch, Blocking, Tx>` directly, or should we store it in a way that allows it to outlive the `'ch` lifetime? The interrupt handler needs to access channel state, but channels have lifetime parameters.
   - **Answer**: `LedChannel` should own the channel directly. The interrupt handler uses direct register access, not the channel object, so lifetime is fine.

2. **Interrupt handler setup**: Should the interrupt handler be set up once globally (current approach) or per-channel? The RMT peripheral has one interrupt handler that can handle multiple channels.
   - **Answer**: Must be global. The `Rmt` peripheral has a single interrupt handler that replaces any previous handler. The handler iterates through all channels to determine which triggered the interrupt. Setting it up in `LedChannel::new()` would work but should only be done once (use a static flag or require it to be set up separately).

3. **Global state consolidation**: Which globals can be moved into `LedChannel`/`LedTransaction`, and which must remain global? The interrupt handler needs to access some state, but we want to minimize globals.
   - **Answer**: Create a single atomic global `ChannelState` struct that holds per-channel state needed by the interrupt handler:
     - `frame_complete: AtomicBool` (or array for multi-channel)
     - `led_counter: AtomicUsize` (current position in transmission)
     - `frame_counter: AtomicUsize` (completed frames)
     - `stats_count: AtomicI32`, `stats_sum: AtomicI32` (optional statistics)
   - Move into `LedChannel`: `num_leds`, `led_data_buffer` (as owned `Box<[RGB8]>`), `channel_idx`
   - `LED_COUNTER` can be reset per-transmission (local variable in `start_transmission`)

4. **Multi-channel support**: Should we design for multiple channels from the start, or add that later? There are 2 RMT channels available.
   - **Answer**: Design for multi-channel from the start, but only support channel 0 for now:
     - Use `ChannelState` array: `[ChannelState; 2]` or similar
     - Use a const `RMT_CH_IDX = 0` (or similar name) so it's easy to find and change later
     - Functions that access registers should take `channel_idx` parameter, but can use the const for now
     - `LedChannel` stores `channel_idx: u8` from the start

5. **Backward compatibility**: Should we keep the old `rmt_ws2811_init2()`, `rmt_ws2811_write_bytes()`, and `rmt_ws2811_wait_complete()` functions as wrappers during migration, or remove them immediately?
   - **Answer**: Remove old init functions (`rmt_ws2811_init()`, `rmt_ws2811_init2()`). Keep `rmt_ws2811_write_bytes()` and `rmt_ws2811_wait_complete()` as wrappers during migration (they can use a static to store the channel/transaction), then remove them in final cleanup phase. Update test code to use new API.

6. **Transaction API**: Should `LedTransaction` be `#[must_use]` like esp-hal's transactions? Should it have a `poll()` method for async use, or just `wait_complete()`?
   - **Answer**: Yes, add `#[must_use]` to `LedTransaction` to prevent accidental drops. Only provide `wait_complete()` for now (blocking wait). Skip `poll()` for async - can add later if needed.

7. **Error handling**: How should errors be handled? Should `start_transmission()` return `Result<LedTransaction, Error>`, or can we assume it always succeeds after initialization?
   - **Answer**: Keep it simple for now - `start_transmission()` returns `LedTransaction` directly (no `Result`). Can add error handling later if needed.

## Notes

- The driver is currently working correctly with atomic synchronization
- We want to stop between phases for manual validation
- Everything should compile and be ready for manual testing after each phase
- The final state should have only one clean global (likely for interrupt handler coordination)
