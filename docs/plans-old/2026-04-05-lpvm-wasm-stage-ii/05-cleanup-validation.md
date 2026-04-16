# Phase 5: Cleanup & Validation

## Scope

Final cleanup, warning fixes, validation, plan archival, and commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Cleanup

### 1. Grep for temporary code

Search the git diff for:
- `TODO` comments
- `debug` / `println!` / `console_log!`
- `unwrap()` in non-test code that should be error-handled
- Dead imports / unused variables

Remove or resolve all findings.

### 2. Fix warnings

```bash
cargo check -p lpvm-wasm 2>&1 | grep warning
cargo check -p web-demo --target wasm32-unknown-unknown 2>&1 | grep warning
cargo check -p lps-builtins 2>&1 | grep warning
```

Fix all warnings.

### 3. Format

```bash
cargo +nightly fmt
```

## Validation

### Host

```bash
cargo check -p lpvm-wasm
cargo test -p lpvm-wasm
cargo test -p lpvm-wasm --test runtime_lpvm_call
cargo test -p lpvm-wasm --test runtime_builtin_sin
cargo test -p lpvm-wasm --test compile_roundtrip
```

### Browser

```bash
cargo check -p web-demo --target wasm32-unknown-unknown
# Full browser test: serve and verify rendering
```

### Workspace

```bash
cargo check
cargo test --workspace --exclude fw-esp32 --exclude fw-emu --exclude fw-tests
```

### Existing tests not broken

```bash
cargo test -p lps-builtins
```

## Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-04-05-lpvm-wasm-stage-ii/summary.md`.

Move the plan directory to `docs/plans-done/`.

## Commit

```
feat(lpvm-wasm): add browser runtime and native builtin linking

- Move ensure_builtins_referenced() into lps-builtins
- Replace filesystem builtins .wasm loading with native Func::new dispatch
- Rename runtime/ to rt_wasmtime/, remove runtime feature flag
- Add rt_browser/ with js_sys WebAssembly backend for wasm32 targets
- Update web-demo to use lpvm-wasm end-to-end (drop lps-wasm dependency)
- Remove builtins.wasm fetch; builtins linked natively on both targets
```
