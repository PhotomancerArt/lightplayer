# Phase 8: Cleanup, review, and validation

## Scope of phase

Clean up any temporary code, fix warnings, ensure all code is properly formatted, and validate the complete implementation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Remove all temporary code and TODOs that are no longer needed

## Implementation Details

### 1. Search for temporary code

Search for and remove:

- `TODO` comments that are no longer needed
- `todo!()` macros that should be implemented or removed
- Debug prints that were added during development
- Temporary test code
- Unused imports

### 2. Fix warnings

Run `cargo check` and fix all warnings:

- Unused code
- Dead code
- Unused imports
- Formatting issues

### 3. Run formatter

```bash
cargo +nightly fmt
```

### 4. Documentation

Ensure all public APIs are documented:

- Module-level documentation
- Trait documentation
- Function documentation
- Example code where appropriate

### 5. Verify integration test

Ensure the integration test:

- Builds the test app if needed
- Runs successfully
- Tests all functionality (serial echo, time, yield)

## Validate

Run from workspace root:

```bash
# Format check
cargo +nightly fmt --check

# Check all packages
cargo check --workspace

# Run all tests
cargo test --workspace

# Clippy (if available)
cargo clippy --workspace -- -D warnings

# Build RISC-V32 targets
just build-rv32

# Run integration test specifically
cargo test --package lp-riscv-tools --test integration_fw_emu
```

Ensure:

- All code compiles without warnings
- All tests pass
- Code is properly formatted
- No temporary code remains
- Documentation is complete
- Integration test passes

## Plan Cleanup

Once validation passes:

1. Add a summary of completed work to `summary.md`
2. Move plan files to `docs/plans-done/2026-01-29-emu-serial-time/`

## Commit

Once the plan is complete, and everything compiles and passes tests, commit the changes with a message following the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat(emu): add serial and time support to RISC-V32 emulator

- Add serial input/output buffers with lazy allocation (128KB each)
- Add time tracking using Instant
- Implement yield, serial write/read/has_data, and time syscalls
- Add firmware syscall wrappers for SerialIo and TimeProvider
- Create emu-guest-test-app binary for integration testing
- Add integration test with emulator main loop
- Add build recipe to justfile

Enables integration tests that can run firmware in emulator and
connect it to a client via serial communication.
```
