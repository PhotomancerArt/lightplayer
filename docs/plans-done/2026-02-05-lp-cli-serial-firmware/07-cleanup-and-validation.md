# Phase 7: Cleanup, Review, and Validation

## Scope of phase

Clean up any temporary code, fix warnings, run validation commands, and ensure everything works end-to-end.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Cleanup Temporary Code

Search for and remove any temporary code:

```bash
cd /Users/yona/dev/photomancer/lp2025
grep -r "TODO\|FIXME\|XXX\|HACK" --include="*.rs" lp-cli/ lp-core/lp-client/ lp-core/lp-server/ lp-core/lp-model/ | grep -v "// TODO" | grep -v "todo!()"
```

Review any remaining TODOs and either:
- Remove them if no longer needed
- Keep them if they're legitimate future work (with proper context)

### 2. Fix Warnings

Run cargo check and fix all warnings:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo check --workspace
```

Fix any warnings that appear. Common issues:
- Unused imports
- Unused variables (prefix with `_` if intentionally unused)
- Dead code (remove or add `#[allow(dead_code)]` with reason)

### 3. Format Code

Run cargo fmt:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo +nightly fmt
```

### 4. Run Tests

Run all tests:

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo test --workspace
```

Fix any failing tests.

### 5. Validate End-to-End

Test the complete workflow:

1. **Test serial port detection** (if hardware available):
   ```bash
   cd lp-cli
   cargo run -- dev --push serial:auto <project-dir>
   ```

2. **Test with explicit port** (if hardware available):
   ```bash
   cargo run -- dev --push serial:/dev/cu.usbmodem2101 <project-dir>
   ```

3. **Test with baud rate** (if hardware available):
   ```bash
   cargo run -- dev --push serial:/dev/cu.usbmodem2101?baud=115200 <project-dir>
   ```

4. **Test stop all projects** (with any transport):
   ```bash
   cargo run -- dev --push local <project-dir>
   ```

Note: If hardware is not available, verify that:
- Code compiles
- Parsing works correctly
- Error messages are clear

### 6. Review Documentation

Check that:
- Public APIs are documented
- Error messages are clear
- User-facing messages are helpful

## Validate

Run the following commands to validate everything:

```bash
cd /Users/yona/dev/photomancer/lp2025

# Check compilation
cargo check --workspace

# Format code
cargo +nightly fmt

# Run tests
cargo test --workspace

# Check for common issues
cargo clippy --workspace -- -D warnings
```

Fix all warnings, errors, and formatting issues before considering the plan complete.
