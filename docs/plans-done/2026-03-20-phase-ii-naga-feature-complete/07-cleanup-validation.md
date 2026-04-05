# Phase 7: Cleanup & validation

## Scope

Final cleanup for the Phase II milestone. Ensure all wasm.q32 filetests pass
(or are annotated for known out-of-scope features like arrays, matrices,
structs). Fix warnings. Format. Verify no Cranelift regression.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.

## Implementation Details

### 1. Grep for temporary code

Search the git diff for:

- `TODO` comments — remove or promote to tracked issues
- `Phase I` / `Phase II` references in error messages — clean up
- `debug_assert!`, `println!`, `eprintln!` left from debugging
- `#[allow(unused)]` or `#[allow(dead_code)]` that should be removed
- `panic!("not implemented")` stubs — replace with proper errors

### 2. Annotate remaining unsupported tests

Features intentionally out of scope for Phase II:

- Arrays (`array/`)
- Matrices (`mat2/`, `mat3/`, `mat4/`)
- Structs (`struct/`)
- Some edge cases (const-fold, specific integer builtins)

For each failing test that uses out-of-scope features, add:

```glsl
// unimplemented: wasm
```

Use the `--fix` flag to clean up any unexpected passes:

```bash
scripts/glsl-filetests.sh --target wasm.q32 --fix
```

### 3. Fix warnings

```bash
cargo check -p lps-frontend 2>&1 | grep warning
cargo check -p lps-wasm 2>&1 | grep warning
cargo check -p lps-filetests 2>&1 | grep warning
```

Fix all warnings.

### 4. Format

```bash
cargo +nightly fmt
```

### 5. Verify no Cranelift regression

```bash
scripts/glsl-filetests.sh --target cranelift.q32
```

All existing Cranelift tests must still pass.

### 6. Full wasm.q32 filetest run

```bash
scripts/glsl-filetests.sh --target wasm.q32
```

Target: 0 unexpected failures. All failures annotated as
`@unimplemented: wasm` or `@ignore: wasm`.

### 7. Run all workspace tests

```bash
cargo test
```

Ensure the full workspace passes.

### 8. Web demo smoke test

```bash
just web-demo
```

Verify rainbow renders in browser.

## Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-03-20-phase-ii-naga-feature-complete/summary.md`.

Move plan files to `docs/plans-done/2026-03-20-phase-ii-naga-feature-complete/`.

## Commit

```
feat(glsl-wasm): Naga WASM backend feature complete (Phase II)

- Vectors: Compose, Splat, Swizzle, AccessIndex, scalarized arithmetic
- Math builtins: Expression::Math dispatch (inline + import)
- User function calls: Statement::Call + CallResult
- LPFX builtins: prototype injection + import dispatch
- Control flow: Break, Continue support
- WASM import section: builtins module + env.memory linkage
- Web demo: rainbow.glsl renders end-to-end
- All wasm.q32 filetests passing (excluding arrays/matrices/structs)
```

## Validate

```bash
cargo +nightly fmt
cargo check
cargo test
scripts/glsl-filetests.sh --target wasm.q32
scripts/glsl-filetests.sh --target cranelift.q32
just web-demo
```

All commands succeed. No warnings. No unexpected test failures.
