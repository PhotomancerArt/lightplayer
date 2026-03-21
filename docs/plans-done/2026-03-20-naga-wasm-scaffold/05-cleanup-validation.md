# Phase 5: Cleanup & validation

## Scope

Final cleanup for the Phase I milestone. Ensure everything compiles cleanly,
tests pass, and no temporary scaffolding remains.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.

## Implementation Details

### 1. Grep for temporary code

Search the git diff for:
- `TODO` comments (remove or promote to tracked issues)
- `debug_assert!` or `println!` / `eprintln!` left from debugging
- `#[allow(unused)]` or `#[allow(dead_code)]` that should be removed
- Any `panic!("not implemented")` stubs — replace with proper errors

### 2. Fix warnings

```bash
cargo check -p lp-glsl-naga 2>&1 | grep warning
cargo check -p lp-glsl-wasm 2>&1 | grep warning
cargo check -p lp-glsl-filetests 2>&1 | grep warning
```

Fix all warnings: unused imports, unused variables, dead code.

### 3. Format

```bash
cargo +nightly fmt
```

### 4. Verify no regression in Cranelift tests

The Cranelift backend should be completely unaffected:

```bash
cargo test -p lp-glsl-filetests -- cranelift
```

### 5. Verify WASM scalar tests

```bash
cargo test -p lp-glsl-filetests -- scalar
```

Both `cranelift.q32` and `wasm.q32` targets should pass for all scalar tests.

### 6. Full filetest run

```bash
cargo test -p lp-glsl-filetests
```

Non-scalar tests on `wasm.q32` should either pass (if they happen to work
with the implemented subset) or be annotated with `// unimplemented: wasm`.
No unexpected failures.

### 7. Workspace check

```bash
cargo check
```

Ensure the full workspace builds. The `web-demo` crate depends on
`lp-glsl-wasm` — it will need a minimal update to compile (the public API
changed: `WasmExport` no longer has `signature`). If it's too invasive for
Phase I, add a `// TODO` and ensure it still builds with the changes.

## Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-03-20-naga-wasm-scaffold/summary.md`.

Move plan files to `docs/plans-done/2026-03-20-naga-wasm-scaffold/`.

## Commit

```
feat(glsl-wasm): scaffold Naga-based WASM backend (Phase I)

- Create lp-glsl-naga crate wrapping naga::front::glsl
- Rewrite lp-glsl-wasm to consume naga::Module instead of TypedShader
- Define GlslType/FloatMode in lp-glsl-naga (no lp-glsl-frontend dep)
- Update lp-glsl-filetests wasm_runner for new types
- Scalar arithmetic filetests passing on wasm.q32
- Remove old 32-file codegen tree from lp-glsl-wasm
```

## Validate

```bash
cargo +nightly fmt
cargo check
cargo test -p lp-glsl-naga
cargo test -p lp-glsl-wasm
cargo test -p lp-glsl-filetests -- scalar
cargo test -p lp-glsl-filetests
```

All commands should succeed with no warnings and no test failures.
