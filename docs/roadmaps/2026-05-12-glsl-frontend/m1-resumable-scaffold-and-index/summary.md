# M1 Summary - Resumable Scaffold and Top-Level Index

## What was built

- Added `lp-shader/lps-glsl` as a `no_std + alloc` workspace crate.
- Added the initial `lps_glsl::CompileJob` API, synchronous `compile(...)` wrapper, and
  `index_source(...)` helper.
- Added span, source-map, diagnostic, token, lexer, and top-level-index foundations.
- Added tests proving all current `examples/**/*.glsl` shaders lex and top-level-index.
- Added `rv32lpn.q32` as a filetest target for `lps-glsl` plus the native RV32 backend.
- Routed `rv32lpn.q32` through `lps-glsl`; M1 intentionally reports that body lowering is not
  implemented yet after indexing succeeds.

## Decisions for future reference

#### Target Name

- **Decision:** Use `rv32lpn.q32` for `lps-glsl` plus `lpvm-native`.
- **Why:** It lets filetest summaries compare `rv32n.q32` and `rv32lpn.q32` side by side.
- **Rejected alternatives:** A separate `--frontend` axis; replacing `rv32n.q32`.
- **Revisit when:** `lps-glsl` needs host JIT or WASM side-by-side targets.

#### Compile Job Shape

- **Decision:** Keep `CompileJob` as the short public type inside `lps-glsl`.
- **Why:** The crate name already scopes it; an engine-facing `LpCompileJob` can wrap or alias it
  later if needed.
- **Rejected alternatives:** `LightCompileJob`.
- **Revisit when:** Runtime scheduling APIs need a product-level name outside the frontend crate.

#### M1 Compile Behavior

- **Decision:** M1 compile routes through lexing and indexing, then returns a planned diagnostic
  because body lowering is not implemented yet.
- **Why:** This proves the resumable/filetest seam without pretending LPIR lowering exists.
- **Rejected alternatives:** Stub LPIR output; silent fallback to Naga.
- **Revisit when:** M2 adds typed HIR and first LPIR lowering.

## Validation

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-filetests
cargo check -p lps-filetests-app
```

Optional smoke:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise function/define-simple.glsl
```

The smoke reached the expected M1 compile-fail:

```text
error: lps-glsl body lowering is not implemented yet at bytes 0..0
```
