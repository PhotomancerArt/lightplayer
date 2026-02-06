# Phase 8: Cleanup & Validation

## Scope of Phase

Final cleanup, validation, and performance verification. Remove any temporary code, fix warnings, and ensure everything works correctly.

## Code Organization Reminders

- Remove all TODOs and temporary code
- Fix all warnings
- Ensure code is properly formatted

## Implementation Details

### 1. Search for Temporary Code

Search for TODOs, FIXMEs, and temporary code:

```bash
cd lp-riscv/lp-riscv-emu
grep -r "TODO" src/
grep -r "FIXME" src/
grep -r "TEMP" src/
```

Remove or address all temporary code.

### 2. Fix Warnings

Run cargo check and fix all warnings:

```bash
cd lp-riscv/lp-riscv-emu
cargo check 2>&1 | grep warning
```

Fix all warnings (unused imports, unused variables, etc.).

### 3. Format Code

Run rustfmt on all changed files:

```bash
cd lp-riscv/lp-riscv-emu
cargo +nightly fmt
```

### 4. Run All Tests

Run the full test suite:

```bash
cd lp-riscv/lp-riscv-emu
cargo test
```

Ensure all tests pass.

### 5. Verify Performance

Run benchmarks if available to verify performance improvement:

```bash
# If benchmarks exist
cargo bench
```

Or manually test with a representative workload to verify the fast path has zero logging overhead.

### 6. Check for Dead Code

Check for any dead code that can be removed:

```bash
cargo check --all-targets
```

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo +nightly fmt
cargo check
cargo test
```

Ensure:
- No warnings
- All tests pass
- Code is properly formatted
- No temporary code remains

## Plan Cleanup

Once validation is complete:

1. Add summary to `summary.md`
2. Move plan files to `docs/plans-done/`

## Commit

Once everything is validated, commit with:

```
refactor(emu): implement fast/debug paths with monomorphic generics

- Add LoggingMode trait for compile-time logging control
- Implement decode-execute fusion to eliminate Inst enum overhead
- Split instructions into category files for better organization
- Remove LogLevel::Verbose, simplify to None/Errors/Instructions
- Fast path has zero logging overhead via monomorphic generics
- Logging path supports full runtime logging control

- Create executor/ directory with category-based organization
- Implement arithmetic, immediate, load/store, branch, jump, system categories
- Update run_loops.rs and step_inner() to use new decode_execute<M>()
- Remove old executor.rs after migration complete
```
