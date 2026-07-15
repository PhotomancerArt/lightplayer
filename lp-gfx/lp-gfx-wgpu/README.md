# lp-gfx-wgpu

LightPlayer GPU graphics backend: `GpuGraphics` implements
[`lp-gfx`](../lp-gfx/README.md)'s `LpGraphics` on wgpu at **IEEE f32
semantics** (`ShaderSemantics::F32Gpu`). Browser WebGPU is the first
deployment target; the code is platform-neutral (native wgpu is the future
non-embedded lp-server engine). Productionizes the M3 spike
(`spikes/wgpu-preview-poc`, retired; findings in the GPU-preview m3 report).

## Compile pipeline

The GPU path forks from the CPU path **at the GLSL source**
(`docs/adr/2026-07-09-gpu-path-forks-at-glsl.md`):

```
authored GLSL (byte-identical to what the device compiles)
  + canonical lpfn prelude        (reference scan + dependency closure over
                                   lps_builtins::CANONICAL_GLSL)
  + generated prototypes          (naga glsl-in needs declaration-before-use;
                                   emitted callee-first — glsl-in assigns
                                   function arena slots at first declaration)
  + generated fragment main()     (wraps render(floor(gl_FragCoord.xy)))
  → naga glsl-in
  → bounded-tanh IR pass          (tanh(x) → tanh(clamp(x, -20, 20));
                                   Metal fast-math tanh NaNs for |x| ≳ 89)
  → naga validate → wgsl-out
  → wgpu render pipeline          (fullscreen triangle; one uniform buffer
                                   per shader instance, offsets from naga's
                                   own layout reflection)
```

No pipeline cache: compiles cost ≈26 ms worst-case warm; cards are
independent backends (device sharing belongs to the browser-integration
milestone).

## Semantics contract

Per `docs/adr/2026-07-09-preview-fidelity-tiers.md`, the requested
`ShaderSemantics` tier is honored exactly or compilation fails: this backend
implements `F32Gpu` only and rejects `Q32` with `GfxError::Backend`. It
never silently substitutes float arithmetic for an explicit Q32 request.
Conformance is judged against the f32 LPIR interpreter oracle plus
hold-or-beat divergence bounds vs the authoritative `wasm.q32` path (see
`tests/`).

## Texture backing and readback policy

Logical unorm16 formats are backed by 32-bit-float textures
(`Rgba16Unorm`/`Rgb16Unorm` → `Rgba32Float`, `R16Unorm` → `R32Float`):
WebGPU has no renderable 16-bit-unorm format, and rendering at f32 then
quantizing with the CPU tier's exact packing rule (`trunc(v·65536)`
saturated) at the readback boundary is the spike-proven configuration
behind the parity numbers. Uploads/readbacks round-trip byte-exactly.

**GPU-residency doctrine** (see `lp-gfx/README.md`): transforms on render
products stay behind trait ops (`blend_textures` is the first of the
family — a small fixed pipeline here) so data never leaves the GPU.
`read_back` is for sinks that inherently need bytes:

- **native** — copy + mapped buffer + blocking `device.poll` (bounded; the
  LED-output path can afford it).
- **wasm32** — explicit `GfxError::Backend`: the browser cannot block on a
  map, the gallery never reads back, and probes/wire sinks run on the CPU
  tier. A deferred/async readback API will be designed when a real browser
  consumer appears.

`GpuGraphics::read_back_f32` (native only) additionally exposes the raw
pre-quantization floats for conformance probes (quantization masks
non-finite lanes).

## Compute and sampling

- `compile_compute_shader` delegates to the inner CPU `LpGraphics`
  (compute stays on the CPU tier permanently).
- `LpShader::sample_rgba16` errors, citing the GPU sample-point-pass
  milestone; sample-point/out buffers are CPU-resident vectors until then.
- `sampler2D` texture inputs belong to their own milestone.

## Workspace notes

- Workspace member, **not** in `default-members`: wgpu's dependency tree is
  heavy and the GPU backend is host-optional. Build/test explicitly with
  `cargo test -p lp-gfx-wgpu` (clippy still covers it via `--workspace`).
- GPU tests are adapter-gated: they skip cleanly on hosts without a GPU
  adapter (CI is ubuntu-arm with no adapter; real GPU runs are local).
  `tests/render_parity.rs` writes review PNGs to
  `target/lp-gfx-wgpu-parity/`.
- On `wasm32` the crate enables wgpu's `fragile-send-sync-non-atomic-wasm`
  feature: `LpGraphics: Send + Sync` requires it, and LightPlayer's wasm
  builds are single-threaded (no atomics), which is exactly the case that
  feature is sound for.
