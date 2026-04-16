# Phase 6: Cleanup and validation

## Scope of phase

Remove any temporary code, debug prints, fix warnings, and perform final validation. Ensure the code is clean and ready for production use.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Remove debug prints

Search for and remove any `println!` statements that were added for debugging:
- In `start_transmission_with_state`
- In `rmt_interrupt_handler` (keep essential ones if needed)
- Any other temporary debug output

### 2. Remove TODO comments

Search for TODO comments and either:
- Implement the TODO if it's still needed
- Remove the TODO if it's no longer relevant
- Keep TODO if it's for future work (multi-channel support, etc.)

### 3. Fix warnings

Run `cargo build` and fix any warnings:
- Unused imports
- Unused variables
- Dead code
- etc.

### 4. Verify interrupt handler setup

Ensure interrupt handler is only set up once. Add a static flag if needed:

```rust
static INTERRUPT_HANDLER_SET: AtomicBool = AtomicBool::new(false);

// In LedChannel::new():
if !INTERRUPT_HANDLER_SET.swap(true, Ordering::Acquire) {
    let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
    rmt.set_interrupt_handler(handler);
}
```

### 5. Verify const usage

Ensure `RMT_CH_IDX` is used consistently throughout the code. Search for hardcoded `0` values and replace with `RMT_CH_IDX` where appropriate.

### 6. Documentation

Add doc comments to public types and methods:
- `LedChannel`
- `LedChannel::new()`
- `LedChannel::start_transmission()`
- `LedTransaction`
- `LedTransaction::wait_complete()`

### 7. Verify final state

Ensure only one clean global remains:
- `CHANNEL_STATE: [ChannelState; 2]` - this is the single clean global
- No other global statics (except constants)

## Tests

Run the test to ensure everything still works:

```rust
// test_rmt.rs should use:
let mut channel = LedChannel::new(&mut rmt, pin, NUM_LEDS)?;
loop {
    let tx = channel.start_transmission(&data);
    channel = tx.wait_complete();
    // ...
}
```

## Validate

Build and test:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo build --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Check for warnings:
```bash
cargo build --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt 2>&1 | grep warning
```

Fix any warnings, then flash and test manually:
```bash
cargo run --target riscv32imac-unknown-none-elf -p fw-esp32 --features test_rmt
```

Verify:
- No warnings (or only acceptable ones)
- Test works correctly
- LEDs update correctly
- Code is clean and well-documented
- Only `CHANNEL_STATE` global remains
- `RMT_CH_IDX` constant is used consistently

## Final Checklist

- [ ] All debug prints removed
- [ ] All TODO comments addressed or documented
- [ ] No compiler warnings
- [ ] All public APIs documented
- [ ] Test code uses new API exclusively
- [ ] Old API functions removed
- [ ] Only `CHANNEL_STATE` global remains
- [ ] Interrupt handler setup is idempotent
- [ ] Code compiles and tests pass
- [ ] Manual hardware test confirms LEDs work
