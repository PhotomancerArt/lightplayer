# Phase 6: Cleanup & Validation

## Scope

Final review, remove temporary code, fix warnings, verify all tests pass.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Cleanup

- Grep the git diff for `TODO`, `FIXME`, `HACK`, `dbg!`, `println!`,
  `#[allow(dead_code)]`, or other temporary markers. Remove or resolve them.
- Remove any `#[allow(unused)]` that is no longer needed.
- Verify all doc comments are accurate.
- Verify `q32.rs` inline ops match the old crate's behavior. Cross-reference
  with `lps-cranelift/src/frontend/codegen/numeric.rs` Q32Strategy methods.
- Verify `builtins.rs` `get_function_pointer` covers all BuiltinId variants
  that can be resolved (no panics at runtime for missing arms).

### 2. Formatting

```
cargo +nightly fmt -p lpvm-cranelift
cargo +nightly fmt -p lpir
```

### 3. Warnings

```
cargo clippy -p lpvm-cranelift -- -D warnings
cargo clippy -p lpir -- -D warnings
```

Fix all warnings.

### 4. Full test run

```
cargo test -p lpvm-cranelift
cargo test -p lpir
cargo test -p lps-frontend
cargo test -p lps-wasm
```

All tests pass.

### 5. Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-03-24-lpvm-cranelift-stage-iii/summary.md`.

Move the plan directory to `docs/plans-done/`.

### 6. Commit

```
feat(lpvm-cranelift): Q32 emission, builtins, and import resolution

- Moved FloatMode to lpir crate, renamed Float → F32
- Import resolution: ImportDecl → BuiltinId → Cranelift FuncRef
- Builtin declaration and JIT symbol lookup (lps-builtins)
- Q32 type mapping: IrType::F32 → Cranelift I32
- Q32 constant encoding (Q16.16)
- Q32 inline ops: fneg, fabs, fmin, fmax, ffloor, fceil, ftrunc
- Q32 casts: ftoi/itof via shift+clamp (ported from old crate)
- Q32 comparisons: fcmp → icmp (signed)
- Q32 builtin calls: fadd, fsub, fmul, fdiv, fsqrt, fnearest
- glsl/lpfn import calls via BuiltinId resolution
- End-to-end Q32 tests including sin import
```

## Validate

```
cargo +nightly fmt -p lpvm-cranelift
cargo +nightly fmt -p lpir
cargo clippy -p lpvm-cranelift -- -D warnings
cargo clippy -p lpir -- -D warnings
cargo test -p lpvm-cranelift
cargo test -p lpir
cargo test -p lps-frontend
cargo test -p lps-wasm
```
