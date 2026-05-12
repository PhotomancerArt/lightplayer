# Phase 8: Cleanup and Validation

## Scope of phase

Clean up any temporary code, fix warnings, verify the build, and ensure everything is ready for hardware testing.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Clean up temporary code

Grep for TODOs and temporary code:

```bash
cd lp-fw/fw-esp32
grep -r "TODO" src/
grep -r "FIXME" src/
grep -r "HACK" src/
grep -r "XXX" src/
```

Review each TODO:
- Remove if addressed
- Keep if it's a known limitation
- Update comments if needed

### 2. Fix warnings

Run:

```bash
cd lp-fw/fw-esp32
cargo clippy --features esp32c6 -- -W clippy::all
```

Fix all warnings:
- Unused imports
- Unused variables
- Dead code
- Clippy suggestions

### 3. Fix formatting

Run:

```bash
cd lp-fw/fw-esp32
cargo fmt
```

### 4. Verify build

Run:

```bash
cd lp-fw/fw-esp32
cargo build --features esp32c6 --release
cargo build --features esp32c6,test_rmt --release
```

Expected: Both builds succeed without errors or warnings.

### 5. Review code structure

Verify:
- All modules are properly organized
- No circular dependencies
- All public APIs are documented
- Error handling is consistent

### 6. Update documentation

Ensure:
- Module-level docs are present
- Public functions are documented
- Complex code has inline comments

## Notes

- Some TODOs may remain if they're known limitations (e.g., RMT initialization complexity)
- Warnings about unsafe code are expected (RMT driver uses unsafe for hardware access)
- Test mode may need hardware-specific adjustments (GPIO pin, LED count)

## Validate

Run:

```bash
cd lp-fw/fw-esp32

# Check for TODOs
grep -r "TODO" src/ | grep -v "known limitation" || echo "No TODOs found"

# Fix warnings
cargo clippy --features esp32c6 -- -W clippy::all

# Format code
cargo fmt

# Verify builds
cargo build --features esp32c6 --release
cargo build --features esp32c6,test_rmt --release
```

Expected:
- No unexpected TODOs
- No clippy warnings (except expected unsafe code warnings)
- Code is formatted
- Both builds succeed
