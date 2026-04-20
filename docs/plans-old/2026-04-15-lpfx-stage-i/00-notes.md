# LPFX M1 (CPU path) — Planning notes

Roadmap: `docs/roadmaps/2026-04-15-lpfx/m1-cpu.md`

## Scope of work

Add **`lpfx/lpfx-cpu`** (`no_std + alloc`): compile an effect's `main.glsl`
through `lps-frontend` -> LPIR -> any **`LpvmEngine`** backend (generic),
map `fx.toml` inputs to **uniforms** (`input_` prefix), apply manifest
defaults, allocate **RGBA16** output via `FxEngine::create_texture`, and
expose **`set_input` / `render` / `output`**.

Also add **`FxEngine` / `FxInstance` traits**, **`TextureId`**,
**`TextureFormat`**, and **`CpuTexture`** to the `lpfx` core crate.

Validation target: **`examples/noise.fx`** renders non-trivial pixels on
the host under `cargo test -p lpfx-cpu`.

**Out of scope for M1:** WebGPU, Wasm lpvm browser engine, Dioxus demo
(M2/M3), lp-engine node integration.

## Current state of the codebase

- **`lpfx/lpfx` (M0):** `FxModule::from_sources`, manifest parsing, `FxValue`,
  `ui = { choices = [...] }`. No filesystem load in core; tests use `include_str!`.
- **`lp-core/lp-engine` `CraneliftGraphics`:** `lps_frontend::compile` + `lower`
  -> `CraneliftEngine::compile` -> `CraneliftModule::direct_call("render")` ->
  pixel loop with Q32 args; `VmContextHeader::default()` only -- uniforms not
  wired on that fast path.
- **`lpvm-cranelift`:** `CraneliftModule` + `CraneliftInstance` with
  `LpvmInstance::set_uniform` / `set_uniform_q32`, `init_globals` /
  `reset_globals`, and `DirectCall::call_i32_buf`. Uniform writes use paths
  from `LpsModuleSig::uniforms_type` (`encode_uniform_write`).
- **`lpvm-native`:** RISC-V native JIT backend, used by fw-esp32 on device.
- **Example shader** `examples/noise.fx/main.glsl` uses `lpfn_*` builtins and
  `layout(binding = 0)` per-uniform declarations; entry
  `vec4 render(vec2 fragCoord, vec2 outputSize, float time)`.

## Questions (all decided)

### Q1: Output pixel storage -- type and dependency

**Context:** The roadmap shows `pub fn output(&self) -> &Texture`.
`lp_shared::Texture` lives in `lp-core/lp-shared`.

**Answer: lpfx owns the texture concept.**

- `lpfx` (core crate) defines the cross-backend texture abstraction:
  `TextureFormat` (color depth enum -- RGBA16 default), `TextureId` (opaque
  handle), and the `FxEngine` creates textures.
- `lpfx-cpu` provides an internal `CpuTexture` (RGBA16 pixel buffer, CPU-only
  backing storage). The CPU engine returns pixel data via this type.
- The GPU path (M2) will have its own backing storage behind the same handle
  abstraction.
- **No dependency on `lp-shared`.** This is an anti-corruption layer; eventually
  `lp-shared::Texture` will depend on or be replaced by lpfx's texture types.
- For M1, the CPU texture is a simple `Vec<u8>` with width/height/format,
  plus `set_pixel_u16` / `pixel_u16` accessors. Minimal and purpose-built.

**Status:** Decided.

---

### Q2: Traits `FxEngine` / `FxInstance` in `lpfx` vs concrete types only in M1

**Context:** Long-term design mirrors lpvm traits; GPU path will want parallel
types. Q1 answer means `FxEngine` creates textures -- that's a trait-shaped
API from the start.

**Answer: Define traits in `lpfx` now.**

Minimal traits in `lpfx` core crate:

```rust
pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;

    fn create_texture(&mut self, w: u32, h: u32, fmt: TextureFormat) -> TextureId;
    fn instantiate(&mut self, module: &FxModule, output: TextureId) -> Result<Self::Instance, Self::Error>;
}

pub trait FxInstance {
    type Error: core::fmt::Display;

    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error>;
    fn render(&mut self, time: f32) -> Result<(), Self::Error>;
}
```

- `TextureId`: opaque `u32`-wrapper handle. Standard graphics-API name,
  namespaced as `lpfx::TextureId`. GPU impl maps it to wgpu resources internally.
- `TextureFormat`: `Rgba16` for now, extensible later.
- Texture pixel data access is on the concrete engine (e.g.
  `CpuFxEngine::texture_data`), not on the trait -- GPU path would need readback,
  which is a different API.
- Saves a refactor when M2 adds a second backend.

**Status:** Decided.

---

### Q3: Q32 vs F32 for the Cranelift path in M1

**Context:** `CraneliftGraphics` uses `FloatMode::Q32` and Q32 `fragCoord` /
`outputSize` / `time` arguments to `render`.

**Answer: Q32. It's the standard mode for lp-shader.** Native float is
hypothetically possible for a float-capable platform without a GPU, but that
hasn't happened. No F32 option needed now or for the foreseeable future.
Same `DirectCall::call_i32_buf` pixel loop as `render_direct_call`.

**Status:** Decided.

---

### Q4: Uniform field paths for `set_uniform`

**Context:** `set_uniform` takes a string path (e.g. UBO struct fields). If
uniforms share the same name as manifest inputs, shader authors can't reuse
those names as locals.

**Answer: `input_` prefix convention.**

- Manifest `[input.speed]` -> GLSL `uniform float input_speed;`
- `set_input("speed", ...)` maps to `set_uniform("input_speed", ...)`
- Matches `[input.speed]` -> `input_speed` -- close to the future `input.speed`
  struct path.
- At instantiation, validate that each manifest input has a matching
  `input_<name>` uniform in `LpsModuleSig::uniforms_type`; error on mismatch.
- **Future:** refactor to a `uniform Input { float speed; ... }` struct once
  the compiler supports uniform structs. `set_input("speed")` ->
  `set_uniform("input.speed")` with no manifest change. Tracked in
  `future-work.md`.

**Status:** Decided.

---

### Q5: `FxModule::load` vs `from_sources` in tests; `no_std` constraint

**Context:** Roadmap sample uses `FxModule::load("...")`; M0 has no load.
`LpFs` lives in `lp-shared` and hasn't been extracted yet.

**Answer:**

- **Tests use `include_str!` + `FxModule::from_sources`** -- no filesystem.
- **No `LpFs` extraction in M1.** Filesystem loading is the caller's
  responsibility; `from_sources` is the API. Moving `LpFs` out of
  `lp-shared` is a separate future task.

**Status:** Decided.

---

### Q6: Backend-specific fast-call for the render loop

**Context:** The per-pixel render loop cannot use `LpvmInstance::call()`
(resets globals per call, metadata lookup). It must use the backend-specific
fast path: `DirectCall::call_i32_buf` for Cranelift, the native equivalent
for lpvm-native, etc. This is the same pattern as lp-engine having
`cranelift.rs` and `native_jit.rs` side by side.

**Answer: Feature-gated backend code in a single `lpfx-cpu` crate.**

Same pattern as `lp-engine`: features + target detection.

- `lpfx-cpu` is `no_std + alloc`, with feature flags for backends:
  - `features = ["cranelift"]` -- `dep:lpvm-cranelift` (host JIT, any arch)
  - `features = ["wasm"]` -- `dep:lpvm-wasm` (browser)
  - `features = ["native"]` -- `dep:lpvm-native` (rv32 only,
    `cfg(all(target_arch = "riscv32", feature = "native"))`)
- Backend deps are optional, pulled in by feature.
- Shared logic (compilation pipeline, input mapping, texture management,
  uniform encoding) is feature-independent.
- The render loop is `#[cfg(feature = "...")]`-gated per backend,
  using each backend's fast-call API.
- For M1: implement the `cranelift` feature first (host tests).
  `native` and `wasm` are follow-ups but the structure supports them.
- Default features: `["cranelift"]` for easy host dev.

**Status:** Decided.

## Notes

- Builtin prefix in shaders: `lpfn_*` (post-rename).
- Rename plan directory uses stage `i` for M1 (first implementation stage after
  M0 scaffold).
- `noise.fx/main.glsl` uniforms need renaming: `speed` -> `input_speed`, etc.
- Three backends to support: `lpvm-cranelift` (host), `lpvm-wasm` (web),
  `lpvm-native` (rv32/ESP32). M1 ships `cranelift` only.
