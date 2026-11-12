# M2.4 Runtime Node Truth Pass Notes

## Scope Of Work

M2.4 is a local runtime cleanup/refactor milestone before canonical sync and UI
work resumes. The goal is to make the engine node graph reflect the domain model
we actually want, now that M2.3 authored bindings exist and the old project sync
surface has been gutted.

In scope:

- Remove dead legacy projection hooks from the runtime node surface.
- Make runtime node responsibilities match authored slot bindings:
  - shader produces render data/capability on an `output` slot
  - fixture consumes shader output through its `input` slot and remains a
    demand root
  - fixture owns the concrete full-texture materialization needed for mapping
  - output remains an IO sink/flush boundary
- Clean up runtime node naming and slot naming while directly touched.
- Update local engine/project-loader tests to validate the new runtime flow.
- Update `examples/basic` last, after runtime tests prove the shape.

Out of scope:

- Canonical wire/project sync redesign.
- Client/view/frontend rebuild.
- Runtime node slot-root exposure for sync/UI.
- Artifact/source mutation.
- Formal input/middle/output node taxonomy.
- Texture-node many-to-many materialization/caching support.
- Turning `OutputNode` into a generic consumer.

## User Notes

- This forked chat is the official cleanup/refactoring thread. M3 planning
  continues separately.
- The old nodes are no longer disposable legacy code; keep them and grow them
  into the final runtime nodes.
- `TextureNode` is a meaningful concept for more complex many-to-many
  fixture/visual relationships, but it is not necessary for the MVP render
  flow because there is no first-class texture resource yet.
- M2.4 should use the simpler MVP flow: shader produces a real render product,
  fixture owns full-texture materialization for sampling, and output remains a
  sink.
- There are future render-product categories:
  - on-demand render products: render when asked, eventually possibly partial
  - texture-backed render products: producer-owned texture already exists
- For now, focus on one useful flavor: lazy execution, full-texture only.
- Fixtures are demand roots and always tick to drive dataflow.
- Outputs are IO nodes; they are directly written to and flushed. There is no
  plan for a generic “fixture render product” that outputs consume.
- Example/source changes should happen late in the milestone.

## Current Code State

### Recent Cleanup Already Done

- Runtime tree files now live under `lp-core/lpc-engine/src/node/`.
- `Node` was renamed to `NodeRuntime`.
- `PressureLevel` moved under `lp-core/lpc-engine/src/memory/`.
- `NodeEntry` now wraps `status`, `state`, and `children` in
  `WithRevision`.
- `LpPath`/`LpPathBuf` moved into `lpfs`; `lpc-model` re-exports them.
- `lpc-model/src/resource.rs` was split into `resource/*`.
- `DomainError` was deleted from `lpc-model`.

### Runtime Node Surface

Relevant file: `lp-core/lpc-engine/src/node/node_runtime.rs`

`NodeRuntime` is mostly clean, but still carries legacy-specific hooks:

- `runtime_output_sink_buffer_id()` is active and used for output flushing.
- `primary_render_product_id()` is active in tests/API helpers.
- `fixture_projection_info()` appears dead and should be removed.
- `shader_projection_wire()` appears dead and should be removed.
- The projection structs `FixtureProjectionInfo` and `ShaderProjectionWire`
  should disappear with those hooks.

### Runtime Flow Today

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`

Current behavior:

- `ShaderNode` stores `texture_node_id`.
- `ShaderNode::tick()` resolves width/height from `TextureNode`, renders into
  its own output buffer, replaces a `RenderProductId`, and publishes that
  render product on slot `texture`.
- `TextureNode` only exposes metadata produced slots:
  - `width`
  - `height`
  - `format`
- `FixtureNode` stores both `texture_node_id` and `shader_node_id`.
- `FixtureNode::tick()` resolves width/height from texture, resolves shader
  render product directly from shader, samples/materializes it, and writes lamp
  colors plus output sink data.
- `OutputNode` allocates an output-channel runtime buffer and remains passive.
- `RuntimeServices` flushes dirty output sink buffers after `Engine::tick()`.

This means texture is still not a real consumer/materialization node. Because
there is no first-class texture resource, making texture “own the texture” would
be forced. M2.4 should remove texture from the MVP runtime flow instead of
pretending it owns a resource that does not exist.

### Authored Defs And Loader

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-model/src/nodes/texture/texture_def.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `examples/basic/*.toml`

Current authored shape:

- `ShaderDef` has `bindings`; canonical example binds `output` to
  `bus#visual.out`.
- `TextureDef` has `bindings`; canonical example consumes `input` from
  `bus#visual.out`.
- `FixtureDef` has `bindings.input` and still has `output_loc`.
- `CoreProjectLoader` currently interprets authored bindings enough to infer:
  - shader -> texture through bus/direct binding
  - fixture -> texture through direct binding
  - fixture -> output through `output_loc`
  - fixture -> shader by finding the shader that targets the texture

M2.4 should simplify this: fixture input should resolve to the shader render
product directly. `TextureDef` / `TextureNode` can remain in the codebase if
useful, but examples and loader tests should no longer require texture as the
main visual path.

### Render Product API Today

Relevant files:

- `lp-core/lpc-engine/src/render_product/render_product_store.rs`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/resolver/resolve_host.rs`

Current render product model:

- `RuntimeProduct::Render(RenderProductId)` is an id handle.
- `RenderProduct` trait supports:
  - `sample_batch(&self, ...)`
  - `as_any()`
- `TickContext` supports:
  - `sample_render_product`
  - `with_native_texture_payload`
  - `defer_render_product_replace`
  - `with_runtime_buffer_mut`
- `ShaderNode` currently renders during `tick()` and stores the rendered texture
  in `RenderProductStore`.

Important implication: current `RenderProduct` represents an already-sampleable
product. M2.4 needs a real lazy-execute shader render product so shader output
is not synonymous with a texture-backed resource.

### Tests

There is already a strong local validation surface:

- `cargo test -p lpc-engine node::`
- `cargo test -p lpc-engine engine::`
- `cargo test -p lpc-engine --test runtime_spine`
- `project_loader` tests under `lpc-engine`
- `CoreProjectRuntime` output sink flush tests

One observed cleanup issue while reading current code:

- `core_project_runtime.rs` has test support code that appears duplicated in one
  spot (`fn get` repeated in `SolidFixtureOutputs`). This should be cleaned up
  during M2.4 validation if it is still present.

## Open Questions

### Q1. How Far Should M2.4 Push The Render Product API?

Context: the desired truth is shader produces an on-demand full-texture render
capability. Current `RenderProduct` is already sampleable/materializable and
`ShaderNode` owns rendering/replacement.

User answer: M2.4 needs the real render-product model. If the shader still
creates the texture-backed product, the texture node has no coherent job. Some
cleanup can happen first, but real render products are the heart of this flow.

Updated direction: M2.4 must introduce a render-product abstraction where the
shader produces a lazy render capability and the fixture owns concrete
full-texture materialization for the MVP flow.

This likely means:

- `ShaderNode` owns GLSL compilation and produces an on-demand render product.
- The render product can be asked to render/fill a full texture target for a
  requested size/format.
- `FixtureNode` consumes that product, requests a full-texture render at its
  configured/effective size, then maps/samples it.
- `TextureNode` is not part of the MVP runtime path.

The render-product API should stay narrow for M2.4:

- full-texture render only
- no partial point rendering yet
- no producer-owned texture-backed products yet except transient
  `TextureRenderProduct` values returned from a render request

### Q2. What Should The Slot Names Be?

Context: `ShaderNode` currently exposes `shader_texture_output_path()` returning
`texture`, but authored bindings use `output`. Fixture consumes `input`.

Suggested answer:

- Shader produced slot: `output`
- Fixture consumed slot: `input`
- Fixture demand trigger remains internal convention `in` for now

### Q3. Does TextureNode Stay In The MVP Runtime Flow?

Context: `TextureNode` is important for future many-to-many visual/fixture
relationships, but without a first-class texture resource it does not have a
coherent ownership boundary in the MVP flow.

User answer: choose the simpler MVP flow. Remove texture from the main runtime
flow now if needed, and bring it back later when texture resources or
many-to-many materialization/caching are in scope. Leaving the old texture node
around is acceptable, but it should not be central to M2.4.

### Q4. Can Projection Hooks Be Removed Before The Flow Refactor?

Context: `fixture_projection_info()` and `shader_projection_wire()` look unused
outside trait impls. They are legacy sync leftovers.

Suggested answer: yes. Remove them in the first phase to simplify the runtime
surface before touching flow.

### Q5. Should `primary_render_product_id()` Stay?

Context: `primary_render_product_id()` is still used by `Engine` and shader
tests. It is not obviously legacy projection, but it is still node-specific.

Suggested answer: keep it for M2.4 unless it naturally disappears after texture
becomes the render-product owner. Do not block this milestone on a perfect
resource ownership API.
