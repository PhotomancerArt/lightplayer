# Sampling Fixture Notes

## Scope

- Keep one authored/runtime fixture node kind, `kind = "fixture"`.
- Add a fixture sampling strategy that consumes a `VisualProduct` and produces a `ControlProduct` by sampling shader points directly.
- Keep the existing texture/polygon mapping path as the `texture_area` fixture sampling strategy.
- Reuse/refactor fixture mapping code so authored path data can drive both texture mapping and direct point sampling.
- Add the shader/VM hot path needed to sample many points efficiently without rendering a texture first.
- Validate behavior with focused tests and `examples/basic`.

## Current Codebase State

### Shader Rendering

- `lp-shader/lp-shader/src/synth/render_texture.rs` synthesizes `__render_texture_<format>`.
- `__render_texture_*` loops over pixels, calls the user `render(vec2)`, converts channels with `FtoUnorm16`, and stores packed unorm16 samples.
- `LpsEngine::compile_px` synthesizes the texture function and stores the function name on `LpsPxShader`.
- `LpsPxShader::render_frame` applies uniforms and calls `PxShaderBackend::call_render_texture`.
- `LpvmInstance::call_render_texture` is the backend hot-path API. Implementations exist in:
  - `lp-shader/lpvm-native/src/rt_jit/instance.rs`
  - `lp-shader/lpvm-native/src/rt_emu/instance.rs`
  - `lp-shader/lpvm-emu/src/instance.rs`
  - `lp-shader/lpvm-cranelift/src/lpvm_instance.rs`
  - `lp-shader/lpvm-wasm/src/rt_wasmtime/instance.rs`
  - `lp-shader/lpvm-wasm/src/rt_browser/instance.rs`

### Visual Product Runtime

- `VisualProduct` is a small graph value: node id plus output id.
- `RenderNode` currently supports full texture materialization with:
  - `render_texture`
  - `render_texture_into`
- `ControlRenderContext` can ask the engine to render a visual product into a caller-owned texture.
- `ShaderNode` implements `RenderNode` and owns shader compilation/rendering state.

### Fixture Runtime

- `FixtureNode` currently:
  - resolves `input` as a `VisualProduct`
  - precomputes `PixelMappingEntry` values from `MappingConfig`
  - allocates/reuses a fixture-owned texture
  - asks the shader to render the full texture
  - accumulates mapped texture pixels into channel accumulators
  - writes unorm16 RGB control samples into the output-owned target
- `generate_mapping_points` already builds one logical point per lamp, but uses `f32` normalized coordinates and is immediately expanded into texture/polygon mapping entries.
- `compute_mapping` uses `libm` and `f32` heavily; that is acceptable for the old texture fixture but should not be on the sampling hot path.

### Authored Node Model

- `NodeDef` is the canonical authored enum in `lpc-model`.
- `FixtureDef::KIND` is `"fixture"`.
- The loader branches on `NodeDef` variants and attaches runtime nodes in `ProjectLoader::attach_loaded_nodes`.
- Bindings are authored on defs through `BindingDefs`; fixtures register source binding for `input` and target binding for `output`.

## User Notes

- Preferred authored kind is still `fixture`; sampling is a mode/config choice on the fixture.
- The existing texture/polygon fixture behavior is useful and should be kept for now.
- Mapping code should be reused/refactored as much as practical.
- Sampling must be fast on ESP32:
  - no per-sample `f32` hot path
  - no texture allocation just to sample a shader
  - shader engine should expose a sampling-focused synthetic function
- Native pixel/sample data should stay unorm16.
- This is a good change before new UI because it clarifies the runtime/product model.

## Direction

- Keep one authored/runtime fixture node kind: `kind = "fixture"`.
- Add an explicit fixture visual sampling strategy:
  - direct shader point sampling
  - texture area sampling, matching the current texture/polygon behavior
- Make the strategy easy to switch in TOML without changing the node type.
- Add a compact sample-point representation for the sampling node:
  - precomputed once when mapping/config changes
  - fixed-point normalized shader coordinates, likely Q16.16 in `i32`
  - channel index per lamp
- Add a new synthetic shader entry:
  - likely `__render_samples_rgba16(points_ptr, out_ptr, count) -> void`
  - points are packed `[x_q16, y_q16]`
  - output is packed RGBA16, `count * 4` `u16`s
  - preserve the same per-sample global reset semantics as texture rendering
- Add engine APIs parallel to texture rendering:
  - `VisualSampleRequest`
  - `VisualSampleTarget`
  - `RenderNode::sample_visual_into`
  - `ControlRenderContext::sample_visual_into`
- The sampling fixture renders visual samples into a reusable scratch buffer, then applies fixture brightness/gamma/color-order into the caller-owned control target.

## Open Questions

### Q1: Separate Def Type Or Shared FixtureDef Mode?

Context: a separate `fixture/sampling` node kind would make direct sampling explicit, but it would duplicate most fixture config/runtime behavior and make strategy switching awkward.

Decision: do not add a separate fixture node kind. Keep `kind = "fixture"` and add a strategy field on `FixtureDef`. This avoids duplicating most fixture config/runtime code and allows switching strategies without changing node type.

### Q2: Keep Existing Texture/Polygon Behavior?

Context: Existing `FixtureNode` still provides useful area/polygon sampling behavior and acts as a reference path.

Decision: keep it as one fixture sampling strategy, likely `texture_area`.

### Q3: Coordinate Format For Shader Sampling?

Context: Mapping points are currently authored/produced as normalized `[0, 1]` `f32`, but hot-path sampling should avoid floats.

Decision: precompute normalized Q16.16 points (`i32`) from authored mapping outside the per-frame hot path, pass those directly to the synthetic shader function.

### Q4: Output Buffer Format From Shader Sampling?

Context: Fixtures still need brightness, gamma, and color order. Output control buffers are unorm16.

Decision: shader sampling returns RGBA16 samples, then fixture converts RGB channels to the output-owned unorm16 control buffer. Do not bake fixture color-order/gamma into the shader synthetic function yet.

### Q5: Should `examples/basic` Switch To `fixture/sampling` In This Plan?

Context: The purpose is to validate the new runtime path and see perf. Keeping old fixture tests is still useful.

Decision: yes. Convert `examples/basic` to the new direct sampling strategy near the end after tests prove the path.

### Q6: What Should The Sampling Strategies Be Called?

Context: The fixture remains one node kind, but its visual evaluation strategy should be explicit.

Suggested answer:

```toml
[sampling]
kind = "direct"
```

and:

```toml
[sampling]
kind = "texture_area"
render_size = { width = 16, height = 16 }
sample_diameter = 2.0
```

`direct` is short and clear for the ESP32-optimized point-sampling path. `texture_area` describes the existing behavior: render to texture, then area-sample mapped points from that texture.

## Out Of Scope

- Removing the existing texture fixture.
- General anti-alias/supersampling for direct sampling.
- Shader debugger/probe UI.
- Direct shader-to-control rendering that knows fixture color order or output layout.
- Reworking all fixture mapping authoring vocabulary.
- New final UI work.
