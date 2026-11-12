# Phase 7: Cleanup, Review, and Validation

## Scope of phase

Final cleanup phase: remove temporary code, fix warnings, run all tests, and create summary documentation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Remove temporary code and TODOs

Search for TODOs and temporary code:

```bash
cd lp-fw
grep -r "TODO" --include="*.rs" .
grep -r "FIXME" --include="*.rs" .
grep -r "XXX" --include="*.rs" .
```

Remove or address all temporary code.

### 2. Fix warnings

Run clippy and fix all warnings:

```bash
cd lp-fw/fw-core
cargo clippy --package fw-core -- -D warnings

cd lp-fw/fw-esp32
cargo clippy --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf -- -D warnings

cd lp-fw/fw-tests
cargo clippy --package fw-tests --features test_usb -- -D warnings
```

### 3. Format code

Run rustfmt on all changed files:

```bash
cd lp-fw
cargo +nightly fmt
```

### 4. Run all tests

```bash
# fw-core tests
cd lp-fw/fw-core
cargo test --package fw-core

# fw-esp32 compilation check
cd lp-fw/fw-esp32
cargo check --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf

# fw-tests compilation check
cd lp-fw/fw-tests
cargo check --package fw-tests --features test_usb
```

### 5. Create summary documentation

Create `lp-fw/fw-esp32/src/tests/README.md`:

```markdown
# Test USB Serial

## Overview

The `test_usb` feature provides comprehensive testing of USB serial connection/disconnection scenarios. It verifies that the firmware continues to operate correctly even when serial is disconnected.

## Architecture

- **Main Loop**: Blinks LED (2Hz), handles messages, increments frame counter
- **MessageRouter**: Decouples main loop from I/O using embassy-sync channels
- **I/O Task**: Handles serial communication, filters `M!` prefix messages
- **Frame Counter**: Atomic counter incremented each main loop iteration

## Message Protocol

Messages use `M!{...}\n` format:
- Commands: `M!{"get_frame_count":{}}\n`, `M!{"echo":{"data":"test"}}\n`
- Responses: `M!{"frame_count":12345}\n`, `M!{"echo":"test"}\n`

The `M!` prefix filters out non-message data (debug prints, etc.).

## Running Tests

### Firmware

Build and flash:
```bash
cd lp-fw/fw-esp32
cargo espflash flash --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf --release
```

### Host-Side Tests

Run automated tests (requires connected ESP32):
```bash
cd lp-fw/fw-tests
cargo test --package fw-tests --features test_usb -- --ignored
```

## Test Scenarios

1. **Start without serial**: Flash → Wait → Connect → Verify frame count increases
2. **Start with serial**: Flash → Connect immediately → Disconnect → Reconnect → Verify
3. **Echo test**: Connect → Echo → Disconnect → Reconnect → Echo → Verify frame count

## Production Integration

The MessageRouter pattern and `M!` prefix filtering are designed to be reusable in the main firmware. The abstractions in `fw-core` can be adapted for production use.
```

## Tests to Write

- Verify all existing tests still pass
- Verify no regressions in SerialTransport
- Verify message protocol works end-to-end

## Validate

Run comprehensive validation:

```bash
# From workspace root
cd /Users/yona/dev/photomancer/lp2025

# Check all packages
cargo check --workspace

# Test fw-core
cd lp-fw/fw-core
cargo test --package fw-core
cargo clippy --package fw-core -- -D warnings

# Check fw-esp32
cd lp-fw/fw-esp32
cargo check --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf
cargo clippy --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf -- -D warnings

# Check fw-tests
cd lp-fw/fw-tests
cargo check --package fw-tests --features test_usb
cargo clippy --package fw-tests --features test_usb -- -D warnings

# Format all code
cargo +nightly fmt --all
```

Ensure:
- ✅ All code compiles without warnings
- ✅ All tests pass
- ✅ No temporary code or TODOs remain
- ✅ Code is properly formatted
- ✅ Documentation is updated
- ✅ Summary is created

## Plan Cleanup

Once validation passes, add summary to `docs/plans/2026-02-05-test-usb-serial/summary.md` and move plan to `docs/plans-done/`.
