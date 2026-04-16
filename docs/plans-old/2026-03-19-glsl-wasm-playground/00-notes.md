# GLSL → WASM Playground: Planning Notes

## Scope of work

Phase iv of the GLSL → WASM playground roadmap (`docs/roadmaps/2026-03-13-glsl-wasm-playground/`):
build a browser-based playground that compiles GLSL to WASM and renders shader output to a canvas.
Target: rainbow.shader renders in a browser with no server.

Predecessor: `docs/plans-done/2026-03-19-glsl-wasm-builtins2/` — rainbow.shader compiles via
`glsl_wasm()`, links with `lps_builtins_wasm.wasm` + shared `env.memory`, and runs under
wasmtime.

## Current state

### Compiler → WASM feasibility

- `lps-frontend` is `#![no_std]` with `extern crate alloc`. All deps (`glsl` parser,
  `hashbrown`, `log`, `libm`, `lps-builtin-ids`) are no_std-compatible.
- `lps-wasm` is `#![no_std]` with `extern crate alloc`. Deps: `lps-frontend`,
  `lps-builtin-ids`, `wasm-encoder`, `glsl`, `hashbrown`, `log`.
- `wasm-encoder` (v0.245): needs verification that it compiles to `wasm32-unknown-unknown`. It's
  pure Rust byte manipulation so should be fine, but may need `default-features = false`.
- The `glsl` parser is from a custom fork (`light-player/glsl-parser`, `feature/spans` branch), used
  with `default-features = false` (no std). Uses `nom`/`nom_locate` with alloc.

### Builtins WASM

- `lps-builtins-wasm` is a `cdylib` that re-exports all `__lp_*` symbols from
  `lps-builtins`.
- Built via `cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release`.
- `build.rs` adds `--import-memory` so the module imports `env.memory`.
- Output: `target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm`.
- Build script: `scripts/build-builtins.sh`.

### Linking pattern (from wasmtime tests)

1. Create shared `WebAssembly.Memory`
2. Instantiate builtins with `{ env: { memory } }`
3. Instantiate shader with `{ builtins: builtinsInstance.exports, env: { memory } }`
4. Call shader functions via `shaderInstance.exports.main(...)`

### What doesn't exist yet

- No `lp-app/` directory
- No playground crate
- No wasm-pack or wasm-bindgen usage anywhere in the codebase
- No web-related code

### Shader main signature (WASM)

`main(vec2 fragCoord, vec2 outputSize, float time) → vec4`

In Q32 WASM: `(i32, i32, i32, i32, i32) → (i32, i32, i32, i32)` — 5 params, 4 multi-value results.

## Questions

### Q1: Multi-value returns from WASM to JS

**Context:** The shader `main` returns `vec4`, emitted as 4 i32 return values (WASM multi-value).
Wasmtime handles this fine. In the browser, the WebAssembly JS API needs to handle multi-value
returns when calling shader functions from JS.

Multi-value was standardized in the WebAssembly spec and is supported in Chrome 85+, Firefox 78+,
Safari 14.1+ (all from 2020-2021). The JS API is supposed to return multiple values, but the exact
API behavior (array? individual values?) needs verification.

**Options:**

- (a) Rely on browser multi-value support (functions return arrays in JS). Simplest — no codegen
  changes.
- (b) Add a "write results to linear memory" wrapper for `main` — the shader writes vec4 to a known
  memory offset, JS reads it. Avoids multi-value entirely.
- (c) Generate a JS-side wrapper that handles multi-value extraction.

**Decision:** (a) — rely on browser multi-value. Well-supported in 2026. Fall back to (b) only if
issues arise.

### Q2: Rendering loop — JS or WASM?

**Context:** The design doc says "requestAnimationFrame loop: call shader main() per pixel, write to
ImageData." At 64×64 = 4096 pixels, that's 4096 JS→WASM function calls per frame.

WASM function call overhead from JS is typically < 100ns. 4096 × 100ns ≈ 0.4ms, well within a 16ms
frame budget. But with the shader body execution, it could add up.

**Options:**

- (a) JS pixel loop: JS iterates pixels, calls `shader.exports.main(...)` each time, writes to
  ImageData. Simple.
- (b) WASM render loop: A Rust function in the playground crate that iterates pixels in WASM, calls
  the shader internally, writes to a shared memory buffer. JS just reads the buffer. Faster, but
  requires linking the shader into the render loop (complex).

The challenge with (b) is that the shader WASM module is dynamically compiled — the playground crate
can't call it directly from Rust. The rendering loop would need to be a separate WASM module that
imports the shader's exports, which requires re-instantiation on every compile. Or we'd need to use
JS as the glue between the render loop module and the shader module.

**Decision:** (a) — JS pixel loop for now. Eventually a more powerful Rust abstraction will replace
this, but we're not there yet. 4096 calls/frame is fast enough for the POC.

### Q3: Playground crate location and build

**Context:** The roadmap specifies `lp-app/playground/` with wasm-pack. This creates a new workspace
member. wasm-pack builds Rust → WASM with wasm-bindgen JS glue, outputting a `pkg/` directory with
`.wasm` + JS modules.

The playground crate would depend on `lps-frontend` and `lps-wasm` only (no Cranelift).

**Options:**

- (a) `lp-app/playground/` — new directory as specified in roadmap
- (b) `playground/` at workspace root — simpler path
- (c) `lp-shader/lps-playground/` — alongside other lps crates

**Decision:** `lp-app/web-demo/`. Application-level code belongs outside `lp-shader/`. `web-demo` is
more descriptive than `playground`.

### Q4: Default shader and UX

**Context:** The playground needs initial content. Rainbow.shader is the target demo but it's 111
lines. For first-time experience, should the textarea start with the full rainbow.shader or
something simpler?

**Decision:** Pre-load rainbow.shader. Auto-compile on change (no manual button). Show errors in the
output panel; keep rendering the last successful compilation while the user edits.

### Q5: Builtins WASM distribution

**Context:** The playground needs `lps_builtins_wasm.wasm` available in the browser. Options:

- (a) Fetch it as a separate file alongside the HTML (`fetch('builtins.wasm')`)
- (b) Embed it in the compiler WASM module (include_bytes at compile time)
- (c) Embed it base64-encoded in the HTML/JS

**Decision:** (a) — fetch as a separate file. Simple, independently cacheable. Build step copies it
to the output directory.

### Q6: Development workflow

**Context:** During development, we need to build the compiler WASM, the builtins WASM, and serve
the playground. This involves:

1. `cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release`
2. `wasm-pack build lp-app/playground/ --target web`
3. Copy `builtins.wasm` to the output directory
4. Serve the playground directory

**Decision:** `just web-demo` recipe that builds everything and serves via `miniserve` (
`cargo install miniserve` — Rust-native static file server). Also `just web-demo-build` for
build-only. Fallback to `python3 -m http.server` if miniserve not available.
