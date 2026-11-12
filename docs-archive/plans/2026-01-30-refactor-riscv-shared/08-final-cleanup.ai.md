# Phase 8: Final Cleanup and Validation

## Scope of phase

Remove temporary code, fix warnings, ensure everything compiles and tests pass, and finalize the
refactoring.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Remove temporary code

Search for and remove:

- `todo!()` macros (except known issues documented)
- Debug `println!` statements
- Commented-out code
- Unused imports
- Temporary test code

### 2. Fix warnings

Run clippy and fix all warnings:

```bash
cargo clippy --package lp-riscv-emu-shared
cargo clippy --package lp-riscv-emu-guest
cargo clippy --package lp-riscv-tools
cargo clippy --package lp-riscv-emu-guest-test-app
cargo clippy --package fw-emu
```

### 3. Format code

```bash
cargo fmt --package lp-riscv-emu-shared
cargo fmt --package lp-riscv-emu-guest
cargo fmt --package lp-riscv-tools
cargo fmt --package lp-riscv-emu-guest-test-app
cargo fmt --package fw-emu
```

### 4. Run all tests

```bash
cargo test --package lp-riscv-tools
cargo test --test integration_fw_emu
cargo test --package lp-riscv-emu-guest-test-app  # if it has tests
```

### 5. Verify integration

- Ensure emulator still works with serial
- Ensure guest code can use serial syscalls
- Ensure tests pass
- Verify no regressions

### 6. Update documentation

- Ensure all public APIs are documented
- Update any relevant README files
- Document the new shared constants location

## Validate

Run from workspace root:

```bash
# Check all packages
cargo check --workspace

# Run all tests
cargo test --workspace

# Run clippy
cargo clippy --workspace -- -D warnings

# Format check
cargo fmt --check --all
```

Ensure:

- All code compiles without errors
- All tests pass
- No warnings
- Code is properly formatted
- No temporary code remains
- Integration tests work
- Documentation is updated
