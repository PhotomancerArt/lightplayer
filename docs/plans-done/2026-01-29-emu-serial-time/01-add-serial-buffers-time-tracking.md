# Phase 1: Add serial buffers and time tracking to emulator state

## Scope of phase

Add serial input/output buffers and time tracking to the `Riscv32Emulator` state. Buffers use lazy
allocation (only allocate when first used) to save memory. Add public methods for host access to
serial buffers.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-tools/src/emu/emulator/state.rs`

Add fields to `Riscv32Emulator`:

- `serial_input_buffer: Option<VecDeque<u8>>` - Input buffer (host → firmware), lazy allocation
- `serial_output_buffer: Option<VecDeque<u8>>` - Output buffer (firmware → host), lazy allocation
- `start_time: Option<Instant>` - Start time for elapsed time calculation (only when std feature
  enabled)

Add imports:

```rust
use alloc::collections::VecDeque;
#[cfg(feature = "std")]
use std::time::Instant;
```

Initialize fields in constructors (`new()` and `with_traps()`):

- Set buffers to `None` (lazy allocation)
- Set `start_time` to `None` (will be initialized on first time syscall)

### 2. Add public methods to `Riscv32Emulator`

Add these methods to the `impl Riscv32Emulator` block:

```rust
/// Drain all bytes from the serial output buffer
///
/// Returns all bytes currently in the output buffer and clears it.
/// Returns empty vector if buffer is not allocated or empty.
pub fn drain_serial_output(&mut self) -> Vec<u8> {
    // TODO: Implement
}

/// Add bytes to the serial input buffer
///
/// Adds bytes to the input buffer, respecting the 128KB limit.
/// If buffer would exceed limit, drops excess bytes from the end.
///
/// # Arguments
/// * `data` - Bytes to add to input buffer
pub fn add_serial_input(&mut self, data: &[u8]) {
    // TODO: Implement
}
```

Implementation notes:

- `drain_serial_output()`: If buffer is `None`, return empty `Vec`. Otherwise, drain all bytes and
  return them.
- `add_serial_input()`: If buffer is `None`, allocate with `VecDeque::with_capacity(128 * 1024)`.
  Add bytes, but if total would exceed 128KB, drop excess from the end (FIFO - keep oldest bytes).

### 3. Add helper methods for buffer access (private)

Add private helper methods that will be used by syscall handlers:

```rust
/// Get or create the serial input buffer
fn get_or_create_input_buffer(&mut self) -> &mut VecDeque<u8> {
    // TODO: Implement
}

/// Get or create the serial output buffer
fn get_or_create_output_buffer(&mut self) -> &mut VecDeque<u8> {
    // TODO: Implement
}

/// Initialize start time if not already initialized
#[cfg(feature = "std")]
fn init_start_time_if_needed(&mut self) {
    // TODO: Implement
}

/// Get elapsed milliseconds since start
///
/// Returns 0 if start time not initialized or std feature disabled.
#[cfg(feature = "std")]
fn elapsed_ms(&self) -> u32 {
    // TODO: Implement
}
```

Implementation notes:

- `get_or_create_input_buffer()`: If `None`, allocate with capacity 128KB, then return mutable
  reference
- `get_or_create_output_buffer()`: Same as above
- `init_start_time_if_needed()`: If `start_time` is `None`, set it to `Instant::now()`
- `elapsed_ms()`: If `start_time` is `Some`, calculate `elapsed().as_millis()` and cast to `u32`.
  Return 0 if `None` or std feature disabled.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-tools
```

Ensure:

- Code compiles without errors
- No warnings (except for unused methods that will be used in next phase)
- State struct initializes correctly
