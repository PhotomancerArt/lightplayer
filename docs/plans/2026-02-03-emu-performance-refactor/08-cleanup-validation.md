# Phase 8: Cleanup, review, and validation

## Scope of phase

Final cleanup, remove any temporary code, fix warnings, ensure everything compiles and tests pass, and validate the implementation.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Remove temporary code

Grep for:
- `TODO` comments
- `FIXME` comments
- `XXX` comments
- Debug `println!` statements
- Unused imports
- Unused functions

Remove or address all temporary code.

### 2. Fix warnings

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check 2>&1 | grep warning
```

Fix all warnings:
- Unused variables
- Unused imports
- Dead code
- Missing documentation
- etc.

### 3. Run formatter

```bash
cd lp-riscv/lp-riscv-emu
cargo +nightly fmt
```

### 4. Verify performance improvements

If possible, run benchmarks or compare before/after performance. The expected improvement is 20-30% from reduced function call overhead.

### 5. Review implementation

Check that:
- `run()` and `run_fuel()` are properly documented
- `run_inner()` implements tight loop with inline fuel checking
- `step_inner()` is marked `#[inline(always)]`
- `run_until_*()` functions use `run()` internally
- `max_instructions` field is completely removed
- All call sites are updated appropriately

### 6. Update documentation

Ensure:
- Module-level docs explain the new fuel-based API
- `run()` and `run_fuel()` have clear doc comments
- Examples in docs are updated if needed

## Tests

Run full test suite:

```bash
cd lp-riscv/lp-riscv-emu
cargo test

# Also test dependent crates
cd ../../lp-core/lp-client
cargo test

cd ../../lp-glsl/lp-glsl-compiler
cargo test
```

## Validate

Run comprehensive validation:

```bash
# Check compilation
cd lp-riscv/lp-riscv-emu
cargo check

# Run tests
cargo test

# Check formatting
cargo +nightly fmt --check

# Check for warnings
cargo clippy -- -D warnings

# Check dependent crates
cd ../../lp-core/lp-client
cargo check
cargo test

cd ../../lp-glsl/lp-glsl-compiler
cargo check
cargo test
```

Ensure:
- All code compiles without warnings
- All tests pass
- Code is properly formatted
- No temporary code remains
- Documentation is updated
- Performance improvements are realized

## Plan cleanup

Once validation is complete:

1. Add summary to `summary.md` (create if needed)
2. Move plan files to `docs/plans-done/2026-02-03-emu-performance-refactor/`

## Commit

Once everything is validated, commit with:

```
refactor(riscv-emu): implement tight loop with fuel-based run API

- Add FuelExhausted variant to StepResult
- Refactor step() to use step_inner() (no fuel check)
- Implement run_inner() with tight loop and inline fuel checking
- Add run() and run_fuel() public API
- Reimplement run_until_*() functions using run()
- Remove max_instructions field (fuel is now per-run)
- Update call sites to use new API

Performance: Expected 20-30% improvement from reduced function call overhead
```
