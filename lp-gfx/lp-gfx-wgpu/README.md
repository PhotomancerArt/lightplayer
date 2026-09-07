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
  → loop-bound IR pass            (a loop with no exit is refused; every
                                   other loop charges the LPVM fuel budget
                                   per back-edge and counts a spent budget
                                   on the shader's fault flag — see below)
  → naga validate → wgsl-out
  → wgpu render pipeline          (fullscreen triangle; one uniform buffer
                                   per shader instance, offsets from naga's
                                   own layout reflection)
```

Diagnostics — naga glsl-in, naga validation, and the unbounded-loop
refusal (`loop_bound_pass`) — render with a `┌─ glsl:LINE:COL` marker in
**authored** coordinates: `assembly::AssembledGlsl` records the byte range
the authored text occupies in the unit, and `wgsl_compile` shifts every
span out of it before rendering (the CPU-tier naga frontend does the same
over its own prefix). A span outside the authored text — prelude, hoisted
declaration copy, texture helper, wrapper `main` — renders location-less.
The Studio parser (`lpa-studio-core` `ui_shader_error.rs`) reads the marker
verbatim.

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

## Loop bounds and the fault flag

The GPU has no fuel meter, and content in this repo is authored against
one (`docs/adr/2026-09-06-gpu-tier-loop-bounds.md`,
`docs/adr/2026-09-07-gpu-tier-spent-budget-faults.md`). `loop_bound_pass`
closes the gap in the naga IR:

1. A loop with no `break`/`return`/`discard` on any path is a
   `GfxError::Compile` naming the assembled-source line (constant `if`
   conditions fold first — glsl-in lowers `while (true)` to
   `if (!true) break;`).
2. Every other loop charges a `var<private>` budget in its `continuing`
   block and `break if`s past `DEFAULT_INVOCATION_FUEL`. The invocation
   that crosses the budget does `atomicAdd(&lp_gfx_loop_fault, 1u)` once
   on a `@group(0)` storage flag the pass binds on the next free slot.
3. `fault_flag` clears the flag before every dispatch, copies it out
   after, and reports a non-zero count as `GfxError::FuelExhausted` with
   `ShaderFuelTrapEntry::Invocations { spent }` — the LPVM's trap type, so
   the shader node routes it to a `Fault` and the outputs paint the fault
   pattern. Native reads it same-frame (a bounded wait on the dispatch's
   own submission; `render` is synchronous natively); the browser reads
   the previous frame's flag before it draws (one frame of latency, a
   capture in flight every frame so a runaway faults continuously).

`tests/loop_fault.rs` spends the budget deliberately on a 4×4 frame and a
few sample points. Nothing in `cargo test -p lp-gfx-wgpu` dispatches an
unbounded loop — the pass is in the only compile path.

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

## Texture inputs (`sampler2D`)

The compile-time `TextureBindingSpec` map arrives through
`ShaderCompileOptions::textures` (the contract shared with the CPU tier;
missing/extra specs are compile errors). Sampling call sites are lowered
at the GLSL source (`src/texture_lowering.rs`): `texelFetch` becomes an
edge-clamping `textureLoad` helper and `texture()` becomes generated
nearest/bilinear arithmetic honoring the spec's per-axis wrap policy and
the `HeightOne` hint (`uv.y` ignored). **Manual sampling is the M5
filtered-sampling decision**: LightPlayer's index-space wrap semantics
(notably the period-`2(n-1)` mirror) and the v0 out-of-range `texelFetch`
clamp are not expressible with WebGPU samplers, and the `textureLoad`-only
path needs no `float32-filterable` feature. At render time,
`LpsValueF32::Texture2D` uniform values (minted by
`GpuGraphics::texture_uniform_value`) resolve through the backend texture
registry into bind-group entries; format and `HeightOne` promises are
re-validated per render, CPU parity.

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
