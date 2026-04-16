# Phase 5: Filetests — engine + module per file

## Scope of phase

Introduce a **per-file** compilation context in `lps-filetests`:

- **One `LpvmEngine` (or backend-specific engine)** per **test file** + target (e.g. `jit.q32`).
- **One `LpvmModule`** per file: compile GLSL → `IrModule` once, then `engine.compile(&ir, &meta)` once.
- **Do not** re-run full compile for each `// expect:` line.

Use a **dispatch enum** (e.g. `FiletestEngine` + `FiletestModule`) if `dyn LpvmModule` is not object-safe; match on backend to call `instantiate()`.

Wire **`run_detail.rs`** / **`compile.rs`** so the module handle is passed into the per-case loop.

## Code Organization Reminders

- New file `engine.rs` (or `filetest_lpvm.rs`) for engine/module context; keep `run.rs` thin.
- Entry points and types first; glue at bottom.

## Implementation Details

- Preserve existing **target selection** (`jit.q32`, `rv32.q32`, `wasm.q32`, `.f32` variants).
- **WASM filetests:** only **`WasmLpvmEngine`** (wasmtime), not browser.
- Store **float mode** in compile options alongside backend.

## Validate

```bash
cargo check -p lps-filetests
cargo test -p lps-filetests --test filetests -- --help
```

Run a **small** filtered filetest subset if integration test exists:

```bash
cargo test -p lps-filetests --test filetests -- scalar/float/from-float --nocapture
```

(Adjust filter to match repo conventions.)
