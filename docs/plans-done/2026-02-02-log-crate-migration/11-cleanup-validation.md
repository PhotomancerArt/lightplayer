# Phase 11: Cleanup & Validation

## Scope of phase

Clean up any temporary code, TODOs, debug prints, and fix all warnings. Validate that the logging infrastructure works correctly across all environments.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Remove Temporary Code

Search for and remove:
- TODO comments related to logging migration
- Debug println! statements that were added for testing
- Temporary workarounds
- Commented-out old code

### 2. Fix Warnings

Run `cargo check --workspace` and fix all warnings:
- Unused imports
- Unused variables (prefix with `_` if needed)
- Unused code (add `#[allow(dead_code)]` if will be used later)
- Dead code warnings

### 3. Verify No Old Code Remains

Search for old patterns:
```bash
grep -r "SYSCALL_DEBUG" --include="*.rs"
grep -r "__host_debug" --include="*.rs"
grep -r "host_println" --include="*.rs"
grep -r "__host_println" --include="*.rs"
```

Should only find:
- Comments explaining the migration
- TODO comments if any remain
- No actual code references

### 4. Format Code

Run formatter:

```bash
cargo +nightly fmt --all
```

### 5. Validate Logging Works

Test logging in each environment:

**Std Applications**:
```bash
RUST_LOG=debug cargo run --bin lp-cli -- <command>
# Verify debug logs appear
```

**Tests**:
```bash
RUST_LOG=debug cargo test --package lp-shared
# Verify test logs appear
```

**Emulator**:
```bash
RUST_LOG=debug cargo run --bin fw-emu
# Verify guest logs appear via syscalls
```

**ESP32**:
- Build and flash firmware
- Verify logs appear on serial output (if possible)

### 6. Update Documentation

Update any documentation that references:
- Old `debug!` macros → `log::debug!`
- Old `host_debug!` → `log::debug!`
- Old `host_println!` → `log::info!`
- `DEBUG=1` env var → `RUST_LOG=debug` or `RUST_LOG=module::path=debug`

### 7. Create Migration Guide

**File**: `docs/plans/2026-02-02-log-crate-migration/MIGRATION_GUIDE.md` (NEW)

Document how to migrate remaining crates:
- Replace `crate::debug!(...)` with `log::debug!(...)`
- Replace `host_debug!(...)` with `log::debug!(...)`
- Replace `host_println!(...)` with `log::info!(...)`
- Remove old macro definitions
- Use `#[test_log::test]` for tests

## Tests

Run all tests to ensure nothing broke:

```bash
cargo test --workspace
```

## Validate

Run from workspace root:

```bash
# Check compilation
cargo check --workspace

# Format code
cargo +nightly fmt --all

# Run tests
cargo test --workspace

# Check for old code patterns
grep -r "SYSCALL_DEBUG\|__host_debug\|host_println\|__host_println" --include="*.rs" | grep -v "//\|TODO\|FIXME"

# Should return no results (or only comments/TODOs)
```

Ensure:
- All code compiles without warnings
- All tests pass
- No old code patterns remain
- Code is properly formatted
- Logging works in all environments
- Documentation is updated
