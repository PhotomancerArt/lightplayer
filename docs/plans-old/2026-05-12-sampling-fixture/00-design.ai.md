# Sampling Fixture Design

## Scope

Add direct shader point sampling as a fixture visual-evaluation strategy.

The fixture remains one authored node kind, `kind = "fixture"`. The new choice
is a fixture sampling strategy:

```toml
[sampling]
kind = "direct"
```

or:

```toml
[sampling]
kind = "texture_area"
render_size = { width = 16, height = 16 }
sample_diameter = 2.0
```

`direct` samples the shader at fixture lamp points and writes control samples.
`texture_area` preserves the current behavior: render the visual product to a
texture, then area-sample texture pixels into fixture channels.

## File Structure

```text
lp-shader/
  lp-shader/src/synth/
    mod.rs
    render_texture.rs
    render_samples.rs
  lp-shader/src/
    engine.rs
    px_shader.rs
  lpvm/src/
    instance.rs
    lib.rs
  lpvm-native/src/rt_jit/instance.rs
  lpvm-native/src/rt_emu/instance.rs
  lpvm-emu/src/instance.rs
  lpvm-cranelift/src/lpvm_instance.rs
  lpvm-wasm/src/rt_wasmtime/instance.rs
  lpvm-wasm/src/rt_browser/instance.rs

lp-core/
  lpc-model/src/nodes/fixture/
    fixture_def.rs
    mapping.rs
    sampling.rs
  lpc-engine/src/products/visual/
    mod.rs
    render_texture_request.rs
    sample_request.rs
    sample_result.rs
  lpc-engine/src/gfx/
    lp_shader.rs
    host.rs
    native_jit.rs
    wasm_guest.rs
  lpc-engine/src/node/
    render_node.rs
    contexts.rs
  lpc-engine/src/nodes/fixture/
    fixture_node.rs
    sampling/
      mod.rs
      direct.rs
      texture_area.rs
      points.rs
    mapping/
      accumulation.rs
      entry.rs
      precompute.rs
      structure.rs
```

## Architecture Summary

### Shader Sampling ABI

`lp-shader` gains a second synthetic materialization function:

```text
__render_samples_rgba16(points_ptr, out_ptr, count) -> void
```

The points buffer is packed as Q16.16 normalized shader coordinates:

```text
[x0_q16, y0_q16, x1_q16, y1_q16, ...]
```

The output buffer is packed RGBA16:

```text
[r0, g0, b0, a0, r1, g1, b1, a1, ...]
```

The synthetic function loops over `count`, loads one point, calls the authored
`render(vec2)`, converts each returned channel with `FtoUnorm16`, and stores
the packed RGBA16 result. It must preserve the same per-sample global-reset
semantics that `__render_texture_*` currently applies per pixel.

Backends expose this through a new `LpvmInstance::call_render_samples` method.
`LpsPxShader` exposes a sampling method that applies uniforms before invoking
the backend hot path.

### Engine Visual Product API

`RenderNode` gains a direct sampling capability alongside texture
materialization. `ControlRenderContext` can request either:

- render visual product into a caller-owned texture
- sample visual product into a caller-owned RGBA16 sample target

`ShaderNode` implements both by dispatching to the compiled shader. Other
render nodes may return a clear unsupported error until they gain support.

### Fixture Sampling Config

`FixtureDef` keeps the shared fixture domain:

- bindings
- mapping
- color order
- transform
- brightness
- gamma correction

It gains:

```rust
pub sampling: FixtureSamplingConfig
```

where `FixtureSamplingConfig` is currently a compact tagged enum:

```rust
enum FixtureSamplingConfig {
    Direct,
    TextureArea,
}
```

`render_size` and `sample_diameter` remain in their existing places for this
slice. Moving strategy-specific fields under `[sampling]` is future cleanup once
we are happy with the runtime path.

### Fixture Runtime Strategies

`FixtureNode` owns shared state and selects a visual evaluation strategy.

`texture_area` owns the current texture target and `PixelMappingEntry` cache.
It should move current texture/polygon code out of the main `fixture_node.rs`
without changing behavior.

`direct` owns:

- compact precomputed sample points
- reusable guest-addressable Q16.16 point buffer
- reusable RGBA16 sample scratch buffer

Direct rendering flow:

1. Resolve fixture input as `VisualProduct`.
2. Precompute sample points when mapping/sampling config changes.
3. Ask the visual product owner to sample those points into RGBA16 scratch.
4. Apply fixture brightness, gamma, and color order.
5. Write directly into the output-owned unorm16 control target.

### Mapping Reuse

The existing mapping point generation already captures the right semantic
thing: one logical point per lamp. This should become shared code under the
fixture sampling module.

The old texture-area path can still expand points into `PixelMappingEntry`
values for area sampling. The new direct path uses the same logical points but
packs them as Q16.16 shader coordinates.

## Main Interactions

```text
OutputNode
  asks Engine to render ControlProduct

Engine
  dispatches ControlProduct to FixtureNode

FixtureNode
  resolves input VisualProduct
  selects sampling strategy

Direct strategy
  precomputes Q16.16 sample points
  asks Engine to sample VisualProduct

Engine
  dispatches VisualProduct to ShaderNode

ShaderNode
  calls LpShader::sample_rgba16

LpsPxShader
  applies uniforms
  calls __render_samples_rgba16

FixtureNode
  maps RGBA16 samples to output-owned control target
```

## Validation Strategy

- First validate the shader sampling ABI independently.
- Then validate the engine visual-product sampling API.
- Then validate fixture direct sampling against simple deterministic shaders.
- Keep current texture fixture tests green.
- Convert `examples/basic` to direct sampling only after the new path is proven.
