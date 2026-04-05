# Phase 8: Cleanup & Validation

## Scope

Final pass: fix warnings, run full validation, ensure code quality, and
verify the lowering against the LPIR validator.

## Tasks

### Warnings
- `cargo clippy -p lpir -p lps-naga -- -D warnings`
- Fix all warnings. Don't suppress them.

### Formatting
- `cargo +nightly fmt -p lpir -p lps-naga`

### LPIR validation
- After lowering, run `lpir::validate::validate_module()` on the output
  `IrModule` and assert it passes.
- Add a test helper that validates after every `compile_and_lower()` call.
- If the validator catches issues, fix the lowering (not the validator).

### Edge cases to audit
- Functions with no return value (void) — ensure implicit `Return {}` is
  emitted.
- Functions with no body statements (only declarations).
- Empty if/else branches.
- Empty loop bodies.
- Nested loops with break/continue targeting correct levels.
- Expressions used multiple times (cache correctness).
- Parameter aliasing: verify that `Store(local, FuncArg)` at the top of
  a function body correctly aliases the VReg.
- Dead code after `return` / `break` / `continue` — lowering should not
  crash but may emit unreachable ops (acceptable).

### Documentation
- Add doc comments to `lower()` entry point explaining the API.
- Add doc comments to `LowerError` variants.
- Ensure `std_math_handler.rs` has a module doc comment.

### Cross-crate test
- Run `cargo test --workspace` to ensure no regressions in other crates.

## Validate

```
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo +nightly fmt --all -- --check
```

Everything green. Stage IV is complete.
