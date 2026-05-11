# Summary

## What was built

- Builtin-first lowering of GLSL `texture(sampler2D, vec2)` for logical `Texture2D` uniforms: `@texture::*` imports, generated builtin IDs/ABI/Cranelift/WASM/native dispatch, and vec4 result-pointer (`out`) ABI consistent with existing LPFN-style builtins.
- Format- and shape-specialized sampler implementations for `Rgba16Unorm` and `R16Unorm`: `texture2d_*` / `texture1d_*` externs with runtime filter and per-axis wrap policy encoded as small integer arguments.
- `General2D` vs `TextureShapeHint::HeightOne`: height-one paths use 1D builtins (`uv.x`, `wrap_x` only); Y and `wrap_y` are intentionally ignored for 1D sampling as documented in tests.
- Wasmtime texture dispatch translates guest texture offsets through linear memory and calls Rust sampler helpers (not raw guest-pointer casts).
- Diagnostics for unsupported sampled-image forms (explicit LOD, missing binding spec, unsupported formats such as `Rgb16Unorm` until a builtin exists).
- Rust reference sampler (`sample_ref`) plus builtin unit tests; GLSL filetests under `filetests/textures/` for nearest/linear, clamp/repeat/mirror-repeat, mixed-axis wrap, R16 vec4 fill, height-one behavior, and negative cases.
- RV32/native/Cranelift fixes for sret texture imports vs vmctx-backed sret calls where needed.

## Decisions for future reference

#### Builtin specialization boundary

- **Decision:** Implement `texture()` via compile-time-selected builtins specialized primarily by storage format (`Rgba16Unorm`, `R16Unorm`) and shape (`General2D` vs `HeightOne`), not by full filter × wrap × format combinations.
- **Why:** Avoids a combinatoric extern/ABI surface while keeping policy runtime inside a small number of sampler bodies.
- **Rejected alternatives:** Fully inlined LPIR for all sampling math (large shaders); one mega-dispatch builtin over all formats/policies (opaque, harder to optimize per format).
- **Revisit when:** Code size or profiling shows hot-path benefit from inlining nearest/clamp-only cases or splitting nearest vs linear builtins.

#### Filter and wrap policy placement

- **Decision:** Pass `TextureFilter` and `TextureWrap` lanes as runtime integer arguments into each format/shape builtin; decode inside shared helpers.
- **Why:** Keeps symbol count manageable; branch cost is small relative to texel loads.
- **Rejected alternatives:** Separate builtins per filter or per wrap pair.
- **Revisit when:** Measurement shows dispatch dominates on RV32.

#### HeightOne vs GLSL `sampler2D`

- **Decision:** Preserve `sampler2D` + `vec2` in source; lowering selects `texture1d_*` builtins and drops `v` / `wrap_y` for height-one bindings.
- **Why:** Matches common GLSL usage for 1D gradients/palettes while allowing a simpler address path on CPU.
- **Rejected alternatives:** Introducing a distinct GLSL `sampler1D`-only surface for M4.
- **Revisit when:** Broader sampler types or API alignment with a future GPU backend warrants it.

#### Filetest expectations for filtered sampling

- **Decision:** Use tolerance-based expectations for bilinear (`Linear`) samples; keep `texelFetch` and nearest-style checks exact where practical.
- **Why:** Q32 fixed-point paths can differ slightly from reference floats; tolerances isolate shader semantics from rounding noise.
- **Rejected alternatives:** Bit-exact linear expectations across all backends.
- **Revisit when:** A single standardized float reference path exists for all targets.

#### CI / tooling note

- **Decision:** Treat `cargo test -p lps-filetests textures` as a filter over Rust tests only; real GLSL coverage uses `scripts/filetests.sh` (the integration harness is `#[ignore]`).
- **Why:** Matches how the repo runs GLSL filetests (`just test-filetests`).
- **Revisit when:** The ignored harness is wired into default `cargo test` (if ever).
