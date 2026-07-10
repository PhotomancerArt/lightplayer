# lp-gfx

LightPlayer graphics abstraction (`no_std + alloc`): the seam between the
engine (`lpc-engine`) and shader execution backends.

## What lives here

- **Traits**: `LpGraphics` (compile + resource ownership), `LpShader`
  (compiled visual shader), `LpComputeShader` (compiled serial compute
  shader). All object-safe; the engine holds `Arc<dyn LpGraphics>` /
  `Box<dyn LpShader>`.
- **Opaque RAII handles**: `TextureHandle`, `SamplePointsHandle`,
  `SampleOutHandle`. Dropping a handle returns the allocation to the backend
  that created it (each handle carries an `Arc<dyn HandleAllocator>` to its
  backend). No manual free calls, no backend pointers in the API.
- **Byte transfer**: `LpGraphics::read_back` yields owned bytes
  (`TextureData`); `create_texture` / `write_texture` /
  `write_sample_points` / `read_sample_out` move bytes the other way. The
  contract is "bytes come back", regardless of where a backend keeps its
  textures resident.
- **`ShaderCompileOptions`** with an explicit `ShaderSemantics` tier
  (`Q32 | F32Gpu`) and **`GfxError`**.

## Backend doctrine

- **One guaranteed CPU backend per target**, cfg-selected
  ([`lp-gfx-lpvm`](../lp-gfx-lpvm/README.md)): `lpvm-native::rt_jit` on
  `riscv32`, `lpvm-wasm::rt_browser` on `wasm32`, `lpvm-wasm::rt_wasmtime`
  elsewhere. This is the product path on embedded targets — it is never
  optional and never feature-gated.
- **Optional accelerated backends** (GPU, `lp-gfx-wgpu`) may additionally be
  constructed on capable targets. Selection happens at runtime creation, by
  the host.
- **Never silent.** A backend must error on options it cannot honor — in
  particular the `ShaderSemantics` tier — rather than substitute different
  semantics. Which tier/backend is active is user-visible state. See
  `docs/adr/2026-07-09-preview-fidelity-tiers.md`.

## Handle lifetime rules

- A handle is only valid with the `LpGraphics` that created it; backends
  reject foreign handles with `GfxError::Backend`.
- Handles keep their backend's memory pool alive (via the allocator `Arc`),
  so dropping the `LpGraphics` before its handles is safe.
- Drop order is release order: on embedded backends the memory returns to
  the shared pool immediately; take-and-drop a cached handle *before*
  allocating its replacement when reallocation pressure matters.

## Naming

Crate `lp-gfx` (traits), impl crates `lp-gfx-lpvm` (CPU) and later
`lp-gfx-wgpu` (GPU). The trait is `LpGraphics`, implementations are
`LpvmGraphics` / `GpuGraphics` — the old `gfx` module / `LpGraphics` trait /
`Graphics` concrete triple-naming is retired.
