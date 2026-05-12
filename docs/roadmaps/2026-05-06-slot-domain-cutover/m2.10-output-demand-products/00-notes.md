# M2.10 Output Demand Products Notes

## Scope

Plan a runtime/data-flow cleanup that replaces the current fixture-to-output
push path with a product/request path similar to render materialization.

The target direction:

- Use `VisualProduct` for graph-level visual material that fixtures consume.
- Introduce a fixture-produced `ControlProduct` for logical device-control
  bytes that outputs consume.
- Make output nodes the demand roots.
- Have output nodes request materialized control data from fixture nodes.
- Separate logical data-flow values from actual data requests/materialization.

This is a plan only. Implementation should happen after the design is stable.

## User Notes

- The current fixture -> output API is awkward and should resemble the render
  product pattern.
- The visual graph product is now named `VisualProduct`.
- Fixture output should be named `ControlProduct`.
- Do not use `DmxProduct` for the graph-level value. The fixture -> output
  boundary is DMX-like `u8` control data, but not necessarily DMX512-shaped.
- Outputs own the mapping from logical control space to protocol/hardware
  shape. E1.31/Art-Net/PixelLite-specific universe packing belongs there.
- Outputs should ask fixtures for data instead of fixtures pushing to outputs.
- Outputs becoming demand roots feels closer to the domain model.
- This is part of a bigger distinction between logical data flow and actual
  data request/materialization.
- Medium-term audio output makes `ChannelProduct` too ambiguous; avoid bare
  channel terminology for the graph product.

## Current State

### Visual path

- `lpc_model::VisualProduct` is a small graph value:
  `node: NodeId`, `output: u32`.
- `LpValue::Product(ProductRef::Visual(VisualProduct))` carries it through slot values.
- `RuntimeProduct::Visual(VisualProduct)` carries it through engine resolution.
- `ShaderState.output` exposes a `VisualProduct` on the shader runtime state
  slot root.
- `RenderNode` is an optional `NodeRuntime` capability. `ShaderNode` implements
  it. This trait may become `VisualNode` or keep the verb-oriented render name.
- `TickContext::render_texture(product, request)` calls the engine session/host,
  which dispatches back to the owning node.
- `FixtureNode` consumes a visual product, then asks the engine to materialize
  a texture with `RenderTextureRequest`.

### Fixture/output path

- `OutputNode` is the demand root.
- `FixtureNode::tick` resolves its `input` visual product and exposes a
  lightweight `ControlProduct`.
- `OutputNode::tick` consumes the `ControlProduct` and asks the fixture to
  render into output-owned `unorm16` samples.
- `OutputNode` allocates an output channel `RuntimeBuffer` and exposes it via
  `runtime_output_sink_buffer_id`.
- `RuntimeServices` registers output sink buffers with `OutputDef` and flushes
  buffers whose revision matches the current engine revision after each engine
  tick.
- `CoreProjectLoader` binds output input and fixture output through normal
  authored `BindingDefs`; fixture no longer names an output directly.

### Binding/authored shape

- `ShaderDef.bindings.output` can bind shader output to a bus/target.
- `FixtureDef.bindings.input` can bind fixture input from a bus/source.
- `FixtureDef.bindings.output` can bind fixture output to a bus/target.
- `OutputDef.bindings.input` can consume fixture control through the resolver.

### Current friction

- Runtime service flushing still mirrors the output-owned control samples into a
  `RuntimeBuffer` for the existing provider API.
- The buffer path mixes logical graph value, materialized control data, and output
  IO storage.
- The visual rename is complete for public product/value/resource names; concrete
  rendered texture payload types still use `TextureRenderProduct`.

## Product Vocabulary Decisions

- `VisualProduct`: graph value for image/visual-field material. Shaders produce
  it; fixtures consume it.
- `ControlProduct`: graph value for logical `u8` device-control data. Fixtures
  or future device nodes produce it; output nodes consume it.
- `ControlRenderTarget`: output-owned materialization target.
- `ControlLayout`: layout hints produced alongside materialization.
- `Dmx`, `E131`, and `ArtNet`: protocol/output-adapter vocabulary. These names
  should appear below `ControlFrame`, not at the graph product boundary.
- `ChannelProduct`: rejected because future audio output makes "channel"
  ambiguous.

## Control Data Shape

- Control data is a logical two-dimensional sample space:
  `row_index -> sample_index -> unorm16`.
- Avoid calling this shape a "universe" in core model names for now. It may map
  cleanly to DMX/E1.31/Art-Net universes, but it can also have more than 512
  samples per row for LED controller outputs that expose larger logical ports.
- `ControlExtent` is the preferred neutral term for the 2D control shape.
- Native LightPlayer control samples are `unorm16`. This gives enough headroom
  for linear RGB, interpolation, and dithering while still mapping exactly to
  final `u8` control protocols when needed.
- The default/canonical LED-friendly protocol packing may still use `510` or
  `512` downstream, but that belongs to output protocol mapping, not the graph
  product itself.
- `ControlLayout` should carry layout hints/ranges so debug UI can show what
  fixture/device region produced which samples.
- Layout hints are metadata only. They should not be required for protocol
  serialization.

## Relevant Files

- `lp-core/lpc-model/src/resource/visual_product.rs`
- `lp-core/lpc-model/src/resource/control_product.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-model/src/slots/visual_product.rs`
- `lp-core/lpc-model/src/slots/control_product.rs`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/node/render_node.rs`
- `lp-core/lpc-engine/src/node/node_call.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/resolver/resolve_host.rs`
- `lp-core/lpc-engine/src/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`
- `lp-core/lpc-model/src/nodes/output/output_def.rs`

## Open Questions

### Q1. Product naming: rename `RenderProduct` now?

Context:

- The current product is produced by shaders and consumed by fixtures.
- It is not a rendered texture; it is a logical handle that can satisfy visual
  materialization requests.
- User suggested `VisualProduct`.

Answer:

Yes. The user has already started the `RenderProduct` -> `VisualProduct`
refactor. This milestone should finish any remaining naming cleanup, while
allowing request verbs such as `render_texture` to remain if they describe the
operation rather than the product.

### Q2. What should fixture output product be called?

Context:

- The user suggested `ControlProduct` or `DmxProduct`.
- Actual output data today is ordered RGB channel samples, not necessarily DMX.
- Future outputs may include GPIO, e1.31, ArtNet, and other control protocols.

Answer:

Use `ControlProduct` for the graph-level logical product, with a concrete
materialized payload named `ControlFrame`. Avoid `DmxProduct` because the
logical output space can be larger or differently packed than DMX universes and
may drive non-light devices such as smoke machines.

### Q3. Should fixture output be a slot value?

Context:

- Shader output is a runtime state slot value.
- Fixture can expose `output: ControlProduct` on runtime state, exactly like
  shader exposes `output: VisualProduct`.
- Output can consume fixture `output` through normal bindings.

Suggested answer:

Yes. Add fixture runtime state with an `output: ControlProduct` value slot.
Fixture tick should publish the product handle, not push buffer data.

### Q4. Who materializes control/channel data?

Context:

- Visual materialization dispatches back to the product owner through
  `RenderNode::render_texture`.
- A similar control path would dispatch back to the fixture through a new
  capability trait.

Suggested answer:

Add an optional `ControlNode` capability on `NodeRuntime` with a request method
such as `render_control(product, request, ctx) -> ControlFrame`. The engine
host validates ownership and dispatches to the product-owning node, matching the
visual path.

### Q5. What should output nodes consume?

Context:

- Output nodes currently own an output buffer and are no-op ticks.
- Fixture currently owns the materialization and pushes into the output buffer.
- Output hardware is the natural demand root.

Suggested answer:

Give `OutputDef` a `bindings.input` convention and have `OutputNode::tick`
resolve its `input` as a `ControlProduct`. The output then asks the engine to
materialize a control frame and writes/flushed it to the provider.

### Q6. Where should hardware flushing live?

Context:

- `RuntimeServices` currently flushes dirty output buffers after engine tick.
- Moving demand to output nodes creates an opportunity for output node tick to
  write directly to `OutputProvider`, but `OutputProvider` lives in
  `RuntimeServices`, not `Engine`.

Suggested answer:

For this milestone, keep provider IO in `RuntimeServices` but make the output
node write the materialized control frame to its own runtime buffer. The service
continues flushing dirty output buffers. This keeps the engine/service boundary
stable while moving dataflow responsibility to output nodes. A later milestone
can move provider IO behind an engine service trait if needed.

### Q7. What happens to `FixtureDef.output_loc`?

Context:

- `output_loc` is the special direct fixture -> output connection.
- Binding-based flow would let output consume fixture output via direct node
  ref or bus ref.

Suggested answer:

Deprecate/remove `FixtureDef.output_loc` in this milestone and add
`OutputDef.bindings`. Update examples so output input is bound from fixture
output, likely via the bus convention if practical. Keep direct node binding as
acceptable for tests.

### Q8. Should output nodes be the only demand roots?

Context:

- Engine supports multiple demand roots.
- Existing tests add fixture demand roots.
- Texture/shader should remain pulled by downstream demand.

Suggested answer:

Yes for the canonical runtime: project loading should add output nodes as demand
roots and stop adding fixtures. Some tests can still use fixture demand roots
when specifically testing fixture behavior, but integration/project tests should
exercise output-root demand.

### Q9. What is the smallest useful implementation slice?

Context:

- This touches model values, slots, runtime product dispatch, fixture node,
  output node, loader, runtime services, and tests.
- Renaming `RenderProduct` to `VisualProduct` is large but mechanical.

Suggested answer:

Split the work:

1. Finish the `VisualProduct` rename cleanup.
2. Add `ControlProduct` and `ControlFrame` model/engine types.
3. Move fixture output from push-buffer side effect to product materialization.
4. Move output nodes to demand roots that consume/materialize `ControlProduct`.

This keeps the work understandable while still cutting through the core
architectural knot.

## Additional Decisions: Control Rendering Into Output-Owned Buffers

- `ControlProduct` should not materialize by allocating and returning an owned
  `Vec<u8>`/`Vec<u16>` frame when the output already owns the destination
  buffer.
- Output nodes should allocate/own the control buffer and pass a mutable render
  target into the fixture/control producer.
- This avoids duplicating output data, which matters on embedded targets.
- `ControlProduct` should advertise a preferred logical extent:
  `ControlProduct { node: NodeId, output: u32, preferred_extent: ControlExtent }`.
- `ControlExtent` should avoid DMX-specific vocabulary. Use neutral axis names
  such as `rows` and `samples_per_row`, unless a better pair emerges during
  implementation.
- Some outputs should autosize from the producer preference, especially simple
  GPIO/microcontroller outputs where the fixture decides how much data exists.
- Other outputs have fixed configured size, such as a PixLite/E1.31 mapping; in
  those cases the output passes its own target size and the fixture/control
  producer must render into that extent, truncating/clipping/filling as needed.
- Control samples are natively `unorm16` at this boundary. Fixtures own
  gamma/color mapping, but outputs own interpolation and dithering, so outputs
  need higher precision before final protocol/hardware quantization.
- Protocol adapters such as E1.31/Art-Net/GPIO convert output-owned `unorm16`
  control buffers into their required wire/hardware format.
