# lp-shader Textures Stage I — Notes

Roadmap: `docs/roadmaps/2026-04-16-lp-shader-textures/m0-lp-shader-crate.md`

## Scope

Create `lp-shader/lp-shader` crate (high-level shader API wrapping
lps-frontend + lpvm) and add `TextureStorageFormat` + `TextureBuffer` to
`lps-shared`. This is M0 of the lp-shader textures roadmap -- no fragment
shader contract, no pixel loop, just the crate scaffold and texture types.

## Current state

### Existing compile pipeline (duplicated in consumers)

Two places do `lps_frontend::compile` + `lower` + `engine.compile`:

1. **lpfx-cpu** (`lpfx/lpfx-cpu/src/compile.rs`):
   `compile_glsl<E: LpvmEngine>(engine, glsl)` -> `CompiledEffect<M>`

2. **lp-engine** (`lp-core/lp-engine/src/gfx/cranelift.rs`):
   inline in `CraneliftGraphics::compile_shader` -- same 3-step pattern

Both also have their own error mapping.

### Crate dependency graph (relevant subset)

```
lps-shared (types, layout, LpsModuleSig)
  |
  +--- lpir (IR, pure, depends on nothing shader-related)
  |
  +--- lpvm (runtime traits, VmContext, memory)
  |      depends on: lps-shared, lpir, lps-q32
  |
  +--- lps-frontend (GLSL -> LPIR via naga)
  |      depends on: lps-shared, lpir, lps-builtin-ids, naga
  |      NOTE: does NOT depend on lpvm
  |
  +--- lpvm-cranelift (backend, depends on lpvm + many cranelift crates)
  +--- lpvm-native (backend, depends on lpvm)
  +--- lpvm-emu (backend, depends on lpvm)
```

### Texture types today

- `lpfx::CpuTexture` (lpfx/lpfx/src/texture.rs) -- Vec<u8> + width/height/format
- `lp_shared::Texture` (lp-core/lp-shared/src/util/texture.rs) -- Vec<u8> + width/height/format
- `lpfx::TextureStorageFormat` -- single variant `Rgba16`
- `lp_model::TextureFormat` -- `Rgb8, Rgba8, R8, Rgba16`

None of these are in lp-shader.

### Crate naming

The `lp-shader/` directory is a workspace directory, not a crate. Existing
crates in it are named `lps-*` (shared types), `lpvm*` (VM runtime),
`lpir` (IR), `lps-frontend` (GLSL parse). The new crate at
`lp-shader/lp-shader/` would have package name `lp-shader`.

## Questions (resolved)

### Q1: Should `lp-shader` be `no_std`?

**Answer**: Yes. `#![no_std]` with `extern crate alloc`. Must be, consistent
with all lp-shader crates.

### Q2: Should `lp-shader` be generic over the backend, or use trait objects?

**Answer**: Generic `<E: LpvmEngine>`. Consumers pick the backend via type
parameter. Matches existing patterns, zero-cost.

### Q3: Where does `CpuTextureBuffer` live?

**Answer**: The `TextureBuffer` trait and shared types (`TextureStorageFormat`)
live in `lps-shared`. The concrete `LpsTextureBuf` implementation lives in
`lp-shader` (see **Q8**). `lps-shared` only needs the trait surface, no lpvm
deps.

### Q4: What error type does `lp-shader` use?

**Answer**: Dedicated `LpsError` enum with `Parse(String)`, `Lower(String)`,
`Compile(String)` variants. `no_std`-compatible, tells callers where it failed.

### Q5: Should `compile()` retain the LPIR module?

**Answer**: Don't retain by default. Backends that keep it (emu) expose via
`LpvmModule::lpir_module()`. May add opt-in retention later for debugging,
but all current test code goes directly to the lpvm layer.

### Q6: Does lp-shader re-export lpvm / lps-shared types?

**Answer**: Re-export lps-shared types (high-level API concepts:
`LpsModuleSig`, `LpsValueF32`, `TextureStorageFormat`, `TextureBuffer`).
Do NOT re-export lpvm types (`LpvmEngine`, `LpvmModule`, `LpvmInstance`) --
those are implementation details.

### Q7: API shape refinements (resolved in discussion)

- **Engine name**: `LpsEngine<E>`, not `LpsShaderEngine` (the 's' in Lps
  is shader).
- **Texture buffer name**: `LpsTextureBuf`, not `CpuTextureBuffer` (Lps
  implies CPU-side).
- **No generic compile**: `LpsEngine` only has `compile_frag(glsl, format)`.
  Fragment is the only shader type for now.
- **`compile_frag` takes output format**: Baked in at compile time for future
  optimization (inlined render functions).
- **Module + instance combined**: `LpsFragShader` holds both internally.
  No separate module/instance concept at the lp-shader level.
- **Uniforms passed into render, not set as state**:
  `shader.render_frame(&uniforms, &mut tex_buf)` where uniforms is
  `LpsValueF32::Struct(...)`. Shader is stateless, instance is internal.
  `render_frame` can take `&self`.
- **Q32 render variant**: Not in M0. Just `LpsValueF32` for now.
  Additive change later if profiling shows it matters (per-frame cost,
  not per-pixel).
- **Threading**: `LpsFragShader` is !Sync (single-threaded render).
  Send depends on backend. Not a concern for M0.

### Q8: Where does `LpsTextureBuf` live?

**Answer**: `lp-shader`. It wraps `LpvmBuffer` (from `lpvm`) + dimensions
+ format, so it can't live in `lps-shared`. `TextureBuffer` trait and
`TextureStorageFormat` stay in `lps-shared` (no deps needed). The trait
has no implementors in `lps-shared` -- that's fine, concrete impl is in
`lp-shader`. `LpsEngine::alloc_texture(w, h, format) -> LpsTextureBuf`.
