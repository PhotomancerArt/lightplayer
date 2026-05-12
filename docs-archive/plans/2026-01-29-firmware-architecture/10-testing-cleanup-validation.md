# Phase 10: Testing, cleanup, and validation

## Scope of phase

Add comprehensive tests, clean up any temporary code, fix warnings, and validate the complete implementation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Remove all temporary code and TODOs that are no longer needed

## Implementation Details

### 1. Add unit tests for fw-core

Add comprehensive tests for `SerialTransport`:

- Test message framing (complete messages)
- Test partial message buffering
- Test multiple messages
- Test invalid JSON handling
- Test error cases

### 2. Add integration tests for fw-emu

Add end-to-end tests that exercise the full stack:

- Test message round-trip
- Test server loop behavior
- Test filesystem operations
- Test output provider

### 3. Clean up temporary code

Search for and remove:

- `TODO` comments that are no longer needed
- `todo!()` macros that should be implemented or removed
- Debug prints
- Temporary test code
- Unused imports

### 4. Fix warnings

Run `cargo check` and fix all warnings:

- Unused code
- Dead code
- Unused imports
- Formatting issues

### 5. Run formatter

```bash
cargo +nightly fmt
```

### 6. Documentation

Ensure all public APIs are documented:

- Module-level documentation
- Trait documentation
- Function documentation
- Example code where appropriate

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app

# Check all packages
cargo check --workspace

# Run all tests
cargo test --workspace

# Format check
cargo +nightly fmt --check

# Clippy (if available)
cargo clippy --workspace -- -D warnings
```

Ensure:

- All code compiles without warnings
- All tests pass
- Code is properly formatted
- No temporary code remains
- Documentation is complete

## Plan Cleanup

Once validation passes:

1. Add a summary of completed work to `summary.md`
2. Move plan files to `docs/plans/_done/2026-01-29-firmware-architecture/`

## Commit

Once everything is complete and validated, commit with:

```
feat(firmware): implement firmware architecture separation

- Create fw-core crate with SerialIo trait and SerialTransport
- Add TimeProvider trait to lp-shared
- Create fw-esp32 app with ESP32-C6 support
- Create fw-emu app with syscall-based providers (stubs)
- Implement ESP32 USB-serial SerialIo
- Implement ESP32 output provider
- Implement server loops for both firmware apps
- Add comprehensive tests
```
