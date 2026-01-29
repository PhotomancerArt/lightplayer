# Phase 8: Cleanup and validation

## Scope of phase

Remove unused code, fix warnings, run full test suite, and update documentation.

## Code Organization Reminders

- Remove unused imports
- Remove or deprecate unused code (like SamplingKernel if no longer needed)
- Fix all warnings
- Ensure code follows project style

## Implementation Details

### 1. Remove unused code

- Check if `SamplingKernel` is still used elsewhere
- If not, mark as deprecated or remove
- Remove unused imports

### 2. Fix warnings

Run:
```bash
cd lp-app && cargo clippy --package lp-engine
```

Fix all warnings:
- Unused variables
- Unused functions
- Dead code
- Style issues

### 3. Fix any remaining TODOs

- Get proper config versions from render context
- Complete RGB channel handling in accumulation
- Extract shared mapping point generation logic if needed

### 4. Run full test suite

```bash
cd lp-app && cargo test --package lp-engine
```

Fix any failing tests.

### 5. Format code

```bash
cd lp-app && cargo +nightly fmt
```

### 6. Update documentation

- Add doc comments where needed
- Update module-level documentation
- Ensure all public APIs are documented

## Validate

Run:
```bash
cd lp-app && cargo check --package lp-engine
cd lp-app && cargo test --package lp-engine
cd lp-app && cargo clippy --package lp-engine
```

Expected:
- Code compiles without warnings
- All tests pass
- No clippy warnings
- Code is properly formatted
