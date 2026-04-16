# Phase 6: Cleanup & Validation

**Done:** This plan directory lives under `docs/plans-done/`; see `summary.md` for outcomes.

## Cleanup

### Temporary code

Grep the crate for any temporary code, TODOs, debug prints, etc.:

```
rg 'TODO|FIXME|HACK|dbg!|println!|eprintln!' lp-shader/lpir/src/
```

Remove or resolve all findings.

### Warnings

```
cargo check -p lpir 2>&1 | grep warning
```

Fix all warnings: unused imports, dead code, missing docs on public items
(if we want `#[warn(missing_docs)]`), etc.

### Formatting

```
cargo +nightly fmt -p lpir
```

Ensure the crate is formatted.

### Code review checklist

- [ ] All public types have `Debug` derives
- [ ] `Op` enum size is ≤ 20 bytes (enforced by test)
- [ ] All spec examples from `docs/lpir/` round-trip through print → parse
- [ ] Interpreter handles all op variants (no panics on valid IR)
- [ ] Validator catches the documented well-formedness violations
- [ ] `#![no_std]` compiles (no accidental `std` usage)
- [ ] No `unwrap()` in library code (only in tests)
- [ ] Error types implement `Display` and `core::error::Error`

## Plan cleanup

### Summary

Write a summary of the completed work to `summary.md` (this folder):

- What was implemented
- Crate structure
- Key design decisions (flat encoding, VRegPool, etc.)
- Test coverage summary
- Any deferred items for Stage III

### Move plan files

Move the plan directory to `docs/plans-done/`:

```
mv docs/plans/2026-03-23-lpir-stage-ii docs/plans-done/2026-03-23-lpir-stage-ii
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

Commit the completed work:

```
feat(lpir): implement lpir crate — core types, builder, printer, parser, interpreter, validator

- Add lp-shader/lpir/ crate (no_std + alloc)
- Flat Op encoding with control flow markers and skip offsets
- VRegPool for variable-arity Call/Return operands
- Stack-based FunctionBuilder with offset patching
- Text format printer matching spec output
- nom-based text format parser with span error reporting
- Round-trip tests for all spec examples
- Interpreter covering all ops, control flow, memory, calls
- Well-formedness validator with positive and negative tests
```
