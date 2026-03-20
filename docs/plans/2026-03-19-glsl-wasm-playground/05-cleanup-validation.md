# Phase 5: Cleanup and validation

## Scope

Final cleanup, warning fixes, formatting, and full validation of the web demo and the rest of the workspace.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Grep for temporary code

Check the git diff for TODOs, debug prints, temporary workarounds:

```bash
git diff --name-only   # list changed files
git diff | grep -E '(TODO|FIXME|HACK|println!|console\.log|debugger)'
```

Remove any that aren't intentional.

### 2. Fix warnings

```bash
cargo build 2>&1 | grep warning
cargo build -p lp-glsl-wasm --target wasm32-unknown-unknown 2>&1 | grep warning
```

Fix all warnings.

### 3. Format

```bash
cargo +nightly fmt
```

### 4. Full workspace validation

```bash
cargo build
cargo test
cargo +nightly fmt --check
just build-fw-esp32
```

All must pass with no regressions.

### 5. Web demo validation

```bash
just web-demo-build
```

Then manually verify in browser:
- Rainbow shader renders correctly
- Auto-compile on edit works
- Error handling works (introduce syntax error, fix it)
- Canvas animation is smooth

### 6. Plan cleanup

Create `summary.md` in the plan directory with:
- What was shipped
- Known limitations
- Follow-ups

Move plan directory to `docs/plans-done/`.

## Validate

```bash
cargo build
cargo test
cargo +nightly fmt --check
just build-fw-esp32
just web-demo-build
```

## Commit

```
feat(web-demo): GLSL → WASM browser demo

- lp-app/web-demo crate: wasm-bindgen API exposing compile_glsl()
- Single-page HTML demo: textarea, canvas, auto-compile on change
- JS render loop: per-pixel shader execution, Q32 → RGBA, requestAnimationFrame
- Builtins linking: shared WebAssembly.Memory, same pattern as wasmtime tests
- Justfile recipes: web-demo-build, web-demo (build + serve via miniserve)
```
