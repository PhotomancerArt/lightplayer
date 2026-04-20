# M2 — GPU Rendering Path (lpfx-gpu)

Render effects on the GPU via WebGPU, using naga for GLSL → WGSL translation.

## Goal

The same `FxModule` that runs on lpvm (M1) also runs on a real GPU via
WebGPU. The pipeline is: GLSL → naga → WGSL → WebGPU fragment shader.
This validates that our GLSL subset works on both paths.

## Deliverables

### `lpfx/lpfx-gpu` crate

Rust crate using `wgpu` (which maps to WebGPU in the browser). Implements
the same `FxEngine` / `FxInstance` interface shape as `lpfx-cpu`.

```rust
pub struct GpuFxEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuFxEngine {
    pub fn instantiate(
        &self,
        module: &FxModule,
        resolution: (u32, u32),
    ) -> Result<GpuFxInstance, Error>;
}

pub struct GpuFxInstance {
    // Holds: wgpu pipeline, uniform buffer, output texture
}

impl GpuFxInstance {
    pub fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Error>;
    pub fn render(&mut self, time: f32) -> Result<(), Error>;
    pub fn read_output(&self) -> Vec<u8>;  // read back pixels
}
```

### GLSL → WGSL translation

Use naga directly:
1. Parse GLSL via `naga::front::glsl`
2. Validate via `naga::valid::Validator`
3. Write WGSL via `naga::back::wgsl`

This is the same naga that `lps-frontend` uses for the CPU path — just a
different output backend. The WGSL output feeds into `wgpu`'s shader
compilation.

### Uniform mapping

Inputs map to a WebGPU uniform buffer. The layout must match what the
translated WGSL expects. naga's module reflection gives us the uniform
struct layout.

`set_input` writes to a staging buffer; `render` uploads it before the
draw call.

### Render pipeline

Full-screen quad with the effect as a fragment shader. The runtime
provides `fragCoord`, `outputSize`, and `time` as either built-ins or
additional uniforms (depending on how the WGSL translation handles the
`render` function signature — may need a small wrapper).

Output is an RGBA texture that can be displayed on a canvas or read back.

### Key challenge: render function signature

The CPU path calls `render(fragCoord, outputSize, time)` per pixel.
The GPU path runs a fragment shader over a full-screen quad. The
translation needs to:
- Convert the `render` function into a fragment shader entry point
- Map `fragCoord` to the built-in fragment position
- Pass `outputSize` and `time` as uniforms
- Return `vec4` as the fragment output

This may require a small GLSL wrapper or naga IR manipulation. Design
the approach here — it's the trickiest part of this milestone.

### No Q32 emulation

The GPU path uses native float. Results will differ from the CPU Q32
path. This is acceptable for preview. Q32 emulation on GPU is future
work (see future-work.md).

## Dependencies

- M0 (scaffold + manifest + effect on disk)
- `wgpu` crate (add to workspace)
- `naga` WGSL backend (already available via naga)

## Validation

```bash
cargo test -p lpfx-gpu
# Run rainbow-noise.fx on GPU, verify non-black output
# Visual comparison with CPU output (manual or screenshot diff)
```

## Open questions

- How to handle the `render` function → fragment shader translation?
  Options: GLSL wrapper that calls `render`, naga IR rewrite, or require
  effects to be written in a fragment-shader-compatible way from the start.
- wgpu version and feature set for WASM target.
- Texture readback performance (needed for LED output, not just display).
