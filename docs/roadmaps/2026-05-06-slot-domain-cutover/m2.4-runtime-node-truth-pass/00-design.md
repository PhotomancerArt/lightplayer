# M2.4 Runtime Node Truth Pass Design

## Scope Of Work

M2.4 refactors the local runtime node graph so the MVP render flow matches the domain model instead of preserving old texture-node scaffolding.

In scope:

- Remove dead node-specific projection hooks from `NodeRuntime`.
- Introduce a real lazy render product for shader output.
- Make `ShaderNode` produce `output -> RuntimeProduct::Render(shader_product_id)` without eagerly creating a texture-backed output.
- Make `FixtureNode` consume shader output through its authored `input` binding and own the transient full-texture render it needs for mapping.
- Keep `OutputNode` as a special IO sink/flush boundary.
- Simplify loader/runtime wiring and `examples/basic` around shader -> fixture -> output.
- Keep or quarantine `TextureNode`, but do not require it in the canonical MVP flow.

Out of scope:

- Canonical wire/project sync rebuild.
- Client/view/frontend work.
- Runtime slot-root sync exposure.
- Artifact/source mutation.
- First-class texture resources.
- Texture-node many-to-many materialization/caching.
- Turning output into a generic consumer node.

## File Structure

```text
lp-core/lpc-engine/src/
  node/
    node_runtime.rs
    contexts.rs

  render_product/
    mod.rs
    render_product_store.rs
    render_texture_request.rs
    shader_render_product.rs
    texture_product.rs
    sample_request.rs
    sample_result.rs

  runtime_product/
    runtime_product.rs

  nodes/
    shader/
      shader_node.rs
    fixture/
      fixture_node.rs
    output/
      output_node.rs
    texture/
      texture_node.rs      # retained only if still useful, not canonical MVP flow

  project_runtime/
    project_loader.rs
    core_project_runtime.rs
    runtime_services.rs
```

## Architecture Summary

The canonical MVP runtime flow becomes:

```text
ShaderNode.output  ->  FixtureNode.input  ->  OutputNode sink buffer
     |                       |
     |                       +-- owns transient full-texture materialization for mapping
     +-- produces lazy shader render product
```

`ShaderNode` owns shader source/config and exposes a lazy render product. The render product can execute the shader for a requested full texture size/format/time, but it does not itself represent a materialized texture resource.

`FixtureNode` is the demand root. It resolves its `input` binding, expects a render product, requests a full-texture render at the size needed for fixture mapping, then samples/materializes that texture and writes lamp/output sink buffers.

`OutputNode` remains a special IO node. It owns sink buffers and `RuntimeServices` flushes dirty output sinks after `Engine::tick()`.

`TextureNode` is not removed as a concept, but it is removed from the canonical MVP runtime path. A future texture resource/materialization/cache plan can bring it back as the many-to-many visual/fixture boundary.

## Main Components And Interactions

### `NodeRuntime`

`NodeRuntime` should be a runtime execution/resource contract, not a legacy sync projection surface.

M2.4 removes:

- `fixture_projection_info()`
- `shader_projection_wire()`
- `FixtureProjectionInfo`
- `ShaderProjectionWire`

M2.4 keeps:

- `runtime_output_sink_buffer_id()` for output sink flushing
- `primary_render_product_id()` only if still useful for tests/debugging after the flow refactor

### Lazy Render Product API

Add a narrow full-texture render API to the render-product layer.

Expected shape:

```rust
pub struct RenderTextureRequest {
    pub width: u32,
    pub height: u32,
    pub format: lps_shared::TextureStorageFormat,
    pub time_seconds: f32,
}

pub trait RenderProduct {
    fn sample_batch(&self, request: &RenderSampleBatch) -> Result<RenderSampleBatchResult, RenderProductError>;

    fn render_texture(
        &mut self,
        request: &RenderTextureRequest,
        graphics: Option<&dyn LpGraphics>,
    ) -> Result<TextureRenderProduct, RenderProductError>;

    fn as_any(&self) -> &dyn core::any::Any;
}
```

The default implementation can return `RenderProductError::NotRenderable` so texture-backed products keep their sample-only behavior.

The store/context layer should expose a mutable render call so a fixture can render through a `RenderProductId` during tick.

### `ShaderRenderProduct`

`ShaderRenderProduct` is the shader-backed lazy render product.

It should:

- hold GLSL source/config-derived compile options
- compile on first render, not necessarily on node tick
- cache the compiled `LpShader` while memory pressure allows
- render a full texture for `RenderTextureRequest`
- return `TextureRenderProduct`
- retain compilation error text internally if useful for diagnostics

This means `ShaderNode::tick()` no longer eagerly renders texture output.

### `ShaderNode`

`ShaderNode` should:

- allocate/register one `ShaderRenderProduct` in `init_resources`
- produce `RuntimeProduct::Render(shader_product_id)` on slot `output`
- stop storing `texture_node_id`
- stop resolving texture dimensions
- stop replacing its render product with texture-backed output during tick

### `FixtureNode`

`FixtureNode` should:

- store a shader/source node id or binding-resolved source determined by the loader
- resolve its `input` binding during tick
- expect `RuntimeProduct::Render`
- ask the render-product store to render a full texture for the mapping dimensions
- reuse the existing mapping/gamma/output sink logic
- stop storing both texture and shader ids
- stop resolving texture width/height from `TextureNode`

For MVP dimensions, use the best currently available source of truth in fixture config/test setup. If no better dimension exists yet, the loader may carry the old texture size forward as fixture render target metadata during the transition, but the runtime flow should not depend on a texture node.

### `CoreProjectLoader`

The loader should turn authored bindings into the direct MVP runtime graph:

- shader `bindings.output` may target a bus or node slot
- fixture `bindings.input` should resolve to a shader output, directly or via bus
- `output_loc` still resolves fixture -> output sink
- texture artifacts may be loaded/recorded only if still present, but canonical basic flow should not require them

### `examples/basic`

Update last after tests pass.

Expected canonical shape:

```text
basic/
  project.toml
  shader.toml
  fixture.toml
  output.toml
  shader.glsl
```

No `texture.toml` is required for the MVP flow.

## Important Constraints

- Keep this milestone local-runtime focused.
- Do not pull canonical sync/view rebuild into the implementation.
- Prefer deleting stale compatibility surfaces over renaming them into permanence.
- Keep output semantics explicit: outputs are IO nodes, not generic data consumers.
- Keep texture-node future work documented but out of this implementation.
