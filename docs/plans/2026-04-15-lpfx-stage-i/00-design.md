# LPFX M1 (CPU path) -- Design

Roadmap: `docs/roadmaps/2026-04-15-lpfx/m1-cpu.md`

## Scope of work

Add `FxEngine` / `FxInstance` traits and texture types to `lpfx`, then
implement `lpfx-cpu` with feature-gated backends. M1 ships the `cranelift`
feature for host testing; `native` (rv32) and `wasm` follow.

## File structure

```
lpfx/
  lpfx/
    src/
      lib.rs              # UPDATE: add trait + texture modules
      texture.rs          # NEW: TextureId, TextureFormat, CpuTexture
      engine.rs           # NEW: FxEngine trait
      instance.rs         # NEW: FxInstance trait
      error.rs            # existing (no change needed for M1)
      manifest.rs         # existing
      input.rs            # existing
      module.rs           # existing
      parse.rs            # existing
    Cargo.toml            # existing (no new deps)
  lpfx-cpu/
    src/
      lib.rs              # NEW: CpuFxEngine, CpuFxInstance, re-exports
      compile.rs          # NEW: GLSL -> LPIR -> LpvmModule, input validation
      render_cranelift.rs # NEW: #[cfg(feature = "cranelift")] pixel loop
    Cargo.toml            # NEW: lpfx, lpvm, lps-frontend; optional backend deps

examples/
  noise.fx/
    fx.toml               # existing
    main.glsl             # UPDATE: rename uniforms to input_*
```

## Conceptual architecture

```
                      lpfx (core, no_std + alloc)
                     +----------------------------+
                     | FxEngine trait              |
                     | FxInstance trait             |
                     | TextureId / TextureFormat   |
                     | CpuTexture (pixel buffer)   |
                     | FxModule / FxManifest       |
                     | FxValue, FxInputDef, ...    |
                     +-------------+--------------+
                                   |
              lpfx-cpu (no_std + alloc, feature-gated backends)
             +---------------------+---------------------+
             | Shared (always compiled):                  |
             |   compile.rs -- GLSL -> LPIR -> module     |
             |   CpuFxEngine -- texture store, compile,   |
             |                  instantiate               |
             |   CpuFxInstance -- input mapping,           |
             |                   uniform encoding         |
             +--------------------------------------------+
             | #[cfg(feature = "cranelift")]               |
             |   render_cranelift.rs                       |
             |   DirectCall::call_i32_buf pixel loop       |
             +--------------------------------------------+
             | #[cfg(feature = "native")] (future)        |
             |   render_native.rs (rv32 fast-call)        |
             +--------------------------------------------+
             | #[cfg(feature = "wasm")] (future)          |
             |   render_wasm.rs (wasmtime fast-call)      |
             +---------------------+---------------------+
                                   |
                                Caller
                  (lp-engine, fw-esp32, demo app, tests)
```

## Main components

### `lpfx` core additions

**TextureId** -- opaque `u32` wrapper. Created by `FxEngine::create_texture`.
GPU backends map this to their own resource types.

**TextureFormat** -- enum, `Rgba16` for now. Extensible.

**CpuTexture** -- `Vec<u8>` pixel buffer with width/height/format and
`set_pixel_u16` / `pixel_u16` accessors. Lives in `lpfx` (not `lpfx-cpu`)
because the trait-level output will use it for any CPU backend.

**FxEngine trait** -- `create_texture`, `instantiate(module, output_tex)`.

**FxInstance trait** -- `set_input(name, value)`, `render(time)`.

### `lpfx-cpu` crate

**CpuFxEngine** -- holds texture storage (`BTreeMap<TextureId, CpuTexture>`).
`instantiate` compiles the module's GLSL through lps-frontend -> LPIR ->
the backend, validates input-to-uniform mapping, sets defaults.

**CpuFxInstance** -- holds the compiled lpvm module/instance (backend-specific),
input defs, and a reference to the output texture. `set_input` encodes
`FxValue` as Q32 and calls `set_uniform`. `render` runs the Q32 pixel loop
via the backend's fast-call API.

**compile.rs** -- shared compilation pipeline. Takes `&str` GLSL, returns
compiled module + metadata. Validates that each manifest `[input.X]` has a
matching `input_X` uniform.

**render_cranelift.rs** -- `#[cfg(feature = "cranelift")]`. Uses
`DirectCall::call_i32_buf` per pixel, same pattern as
`lp-engine/src/gfx/cranelift.rs::render_direct_call`.

### Input -> uniform mapping

Convention: `[input.speed]` -> GLSL `layout(binding = 0) uniform float input_speed;`

At instantiation, the engine checks `LpsModuleSig::uniforms_type` for each
manifest input -- if `input_<name>` is missing, error. `set_input("speed", v)`
calls `set_uniform("input_speed", v)` internally.

Future: uniform struct `Input { float speed; ... }` replaces prefix convention.

### Cargo.toml pattern (matches lp-engine)

```toml
[features]
default = ["cranelift"]
cranelift = ["dep:lpvm-cranelift"]
native = ["dep:lpvm-native"]
wasm = ["dep:lpvm-wasm"]

[dependencies]
lpfx = { path = "../lpfx" }
lpvm = { path = "../../lp-shader/lpvm", default-features = false }
lps-frontend = { path = "../../lp-shader/lps-frontend", default-features = false }
lpvm-cranelift = { path = "../../lp-shader/lpvm-cranelift", optional = true, default-features = false, features = ["glsl"] }
lpvm-native = { path = "../../lp-shader/lpvm-native", optional = true, default-features = false }
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", optional = true, default-features = false }
```

# Phases

## Phase 1: Texture and trait types in `lpfx`
## Phase 2: `lpfx-cpu` crate scaffold + workspace
## Phase 3: Compilation pipeline
## Phase 4: Cranelift render loop + `set_input`
## Phase 5: Update `noise.fx` + integration test
## Phase 6: Cleanup + validation
