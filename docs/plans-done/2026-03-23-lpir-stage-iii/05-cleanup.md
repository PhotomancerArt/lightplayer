# Phase 5: Cleanup & Validation

## Scope

Final review, cleanup, and commit.

## Cleanup

### Temporary code

Grep the crate for any temporary code, TODOs, debug prints, etc.:

```
rg 'TODO|FIXME|HACK|dbg!|println!|eprintln!' lp-glsl/lpir/src/
```

Remove or resolve all findings.

### Warnings

```
cargo check -p lpir 2>&1 | grep warning
```

Fix all warnings.

### Formatting

```
cargo +nightly fmt -p lpir
```

### Code review checklist

- [ ] All new tests pass
- [ ] No regressions in existing tests
- [ ] Any interpreter/validator bugs found during testing are fixed
- [ ] Test helpers are at the bottom of each test file
- [ ] No debug prints or TODO comments left in test code
- [ ] `#![no_std]` still compiles (no accidental `std` usage)

## Plan cleanup

### Summary

Write a summary of the completed work to `summary.md` (this folder):

- What was added
- Test count before and after
- Bugs found and fixed (if any)
- Coverage summary by category

### Move plan files

Move the plan directory to `docs/plans-done/`:

```
mv docs/plans/2026-03-23-lpir-stage-iii docs/plans-done/2026-03-23-lpir-stage-iii
```

## Validate

Final validation:

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```

All must pass with zero warnings and zero failures.

## Commit

```
test(lpir): comprehensive interpreter and validator test coverage

- Reorganize tests into focused submodules (interp.rs, validate.rs)
- Add interpreter tests for all Op variants (arithmetic, comparison,
  logic, constants, immediates, casts, select, copy)
- Add edge-case numeric tests (div-by-zero, NaN, saturating casts,
  shift masking, wrapping arithmetic)
- Add control flow tests (if/else, loop, switch, break, continue,
  br_if_not, nested loops, early return)
- Add memory tests (slot, load, store, memcpy, dynamic index)
- Add call tests (local, import mock, multi-return, recursion)
- Add stack overflow and error path tests
```
