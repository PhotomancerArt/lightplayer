# Phase 7: Cleanup & Validation

## Scope of Phase

Clean up any temporary code, fix warnings, and validate the complete implementation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Clean up temporary code

Search for and remove:
- TODO comments (unless they're for future work)
- Debug `println!` statements
- Commented-out code
- Unused imports
- Unused variables

```bash
# Search for TODOs
grep -r "TODO" lp-fw/fw-esp32/src/ lp-fw/fw-core/src/transport/

# Search for debug prints
grep -r "println!" lp-fw/fw-esp32/src/ lp-fw/fw-core/src/transport/

# Search for commented code
grep -r "^[[:space:]]*//[[:space:]]*[a-zA-Z]" lp-fw/fw-esp32/src/ lp-fw/fw-core/src/transport/
```

### 2. Fix warnings

Run clippy and fix all warnings:

```bash
cargo clippy --package fw-core -- -D warnings
cargo clippy --package fw-esp32 --features esp32c6,server -- -D warnings
```

### 3. Run tests

Ensure all tests pass:

```bash
# fw-core tests
cargo test --package fw-core

# fw-emu tests (uses SerialTransport)
cargo test --package fw-emu
```

### 4. Verify compilation

Ensure everything compiles:

```bash
# fw-core
cargo check --package fw-core

# fw-esp32
cargo check --package fw-esp32 --features esp32c6,server

# fw-emu (uses SerialTransport)
cargo check --package fw-emu
```

### 5. Verify exports

Ensure all new items are properly exported:

```bash
# Check MessageRouterTransport is exported
grep -r "MessageRouterTransport" lp-fw/fw-core/src/lib.rs lp-fw/fw-core/src/transport/mod.rs

# Check io_task is exported
grep -r "io_task" lp-fw/fw-esp32/src/serial/mod.rs
```

### 6. Documentation

Ensure documentation is up to date:
- Module-level docs
- Function docs
- Type docs

## Validate

Run the following comprehensive validation:

```bash
# Full check
cargo check --workspace

# Full clippy
cargo clippy --workspace -- -D warnings

# All tests
cargo test --workspace

# Format check
cargo fmt --check
```

Ensure:
- All code compiles
- No warnings
- All tests pass
- Code is formatted
- No temporary code remains
- Documentation is complete
