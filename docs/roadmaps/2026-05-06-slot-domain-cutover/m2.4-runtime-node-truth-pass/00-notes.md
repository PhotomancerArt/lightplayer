# M2.4 Runtime Node Truth Pass Notes

> These notes now assume M2.3 (`authored slot bindings`) lands first.
> M2.4 should consume that authored binding model rather than inventing or
> settle it inside the runtime refactor.

## Scope Of Work

M2.4 is a runtime cleanup and refactor milestone before canonical project sync
work resumes. The goal is to make the runtime node graph reflect the domain
model we actually want, now that the old UI/message burden is gone.

In scope:

- Remove leftover legacy projection hooks and node-specific sync shims from the
  runtime node surface.
- Rewire node flow so the runtime graph reflects the actual dataflow, using
  the authored binding model established by M2.3:
  - shader produces a render product on an `output` slot
  - texture consumes that render product and materializes a texture-backed
    runtime buffer / render target
  - fixture consumes texture output through a proper binding/consumed slot
  - output flow uses the established runtime/resource patterns instead of ad hoc
    projection helpers
- Clean up `lpc-engine` module organization and naming where directly touched by
  this refactor.
- Add local unit/integration tests that validate the new runtime flow in
  process.
- Update `examples/basic` and other example/test source artifacts last, after
  the new runtime shape is validated.

Out of scope:

- Canonical wire/project sync redesign.
- Client/view/frontend work.
- Runtime slot root exposure for sync/UI.
- Artifact/source mutation.
- Broad engine API cleanup not directly required for the node truth pass.

## User Notes

- This forked chat is now the official cleanup/refactoring thread; M3 planning
  continues separately.
- The old nodes are no longer considered disposable legacy code. The decision
  is to keep them and grow them into the final ones.
- M2.4 is no longer the place to decide the authored binding language. That is
  M2.3 work; M2.4 should assume bindings already exist on source defs and focus
  on making runtime nodes honor them truthfully.
- The current runtime shape is wrong in a few important ways:
  - `legacy/nodes` was the wrong home for real fixture logic
  - leftover projection hooks in `Node` are legacy sync artifacts
- We should validate the runtime shape with local tests before resuming new
  API/frontend work.
- Example/source updates should happen last; temporary breakage there is fine
  during the refactor.
- Refined direction:
  - keep `TextureNode` for now
  - shaders produce render products
  - `TextureNode` becomes a real render-target node with input and output
  - start with one render-product flavor: on-demand producer, full-texture
    render only
  - shaders do not own textures in this stepping-stone model
  - later, some fixtures may consume render products directly, but that is not
    required now
- Outputs are intentionally different from middle nodes:
  - no planned “fixture render product” abstraction for outputs
  - outputs are directly written to and then flushed
  - fixtures are demand roots and always tick to drive data flow
  - outputs are IO nodes that also tick, but mostly exist as sink/flush
    boundaries
  - future input nodes may also have special runtime behavior
- Longer-term runtime taxonomy may become something like input / middle /
  output node classes, but M2.4 does not need to formalize that yet.

## Current Code State

### Module Layout

- `lp-core/lpc-engine/src/nodes/` has already been partially refactored:
  - `nodes/fixture/*`
  - `nodes/shader/*`
  - `nodes/output/*`
  - `nodes/texture/*`
  - `nodes/placeholder/*`
- The old `legacy/nodes/*` files were moved into `nodes/fixture/*`, and the
  crate root no longer exports `legacy`.
- `nodes/mod.rs` now re-exports the concrete runtime node types.

### Runtime Flow Today

- `ShaderNode` currently depends on a `texture_node_id`.
- `TextureNode` produces width/height/format metadata as produced slots:
  - `width`
  - `height`
  - `format`
- `ShaderNode::tick()` resolves width/height from `TextureNode`, allocates an
  output buffer to match those dimensions, renders into it, and publishes a
  `RuntimeProduct::Render` at produced slot path `texture`.
- `FixtureNode` currently depends on:
  - `texture_node_id` for width/height metadata
  - `shader_node_id` for the shader render product
  - `output_sink` `RuntimeBufferId`
- `FixtureNode::tick()` resolves:
  - width/height from `TextureNode`
  - render product from `ShaderNode`
  then samples the render product and writes directly into:
  - its own lamp-colors runtime buffer
  - the output sink runtime buffer
- `OutputNode` is mostly a passive sink allocator:
  - allocates an output-channels runtime buffer
  - exposes `runtime_output_sink_buffer_id()`
  - does not consume any binding-driven input
- `RuntimeServices` flushes registered output sink buffers after engine tick.

### Loader Wiring Today

- `CoreProjectLoader` still loads `TextureDef` artifacts and attaches a
  `TextureNode` runtime instance for each one before shaders/fixtures.
- `LoadedNodeDef` still includes `Texture(TextureDef)`.
- `ShaderDef` now declares produced output bindings instead of `texture_loc`.
- `TextureDef` now declares a consumed input binding.
- `FixtureDef` now declares a consumed input binding instead of `texture_loc`,
  while `output_loc` remains for output sink registration.
- `CoreProjectLoader` resolves:
  - shader output binding -> texture input binding
  - fixture input binding -> texture node
  - fixture -> output node
  - fixture -> shader node via the texture binding relationship
- This means the source model and runtime flow are both still organized around
  the texture node as an intermediate object, but runtime behavior is still not
  truthful: shader currently renders “through” texture sizing while texture
  itself does not act like a real render-target consumer.

### Node Trait Surface

- `Node` still has these non-core hooks:
  - `runtime_output_sink_buffer_id()`
  - `primary_render_product_id()`
  - `fixture_projection_info()`
  - `shader_projection_wire()`
- `fixture_projection_info()` and `shader_projection_wire()` appear to be dead
  legacy-sync leftovers. Current `rg` only finds them in the trait and node
  impls, not in active sync/projection consumers.
- `runtime_output_sink_buffer_id()` and `primary_render_product_id()` are still
  active through `Engine` and `project_loader`.

### Source Artifacts Today

- `TextureDef` is still an authored node def with `size` and `bindings`.
- `ShaderDef` publishes its `output` slot through `bindings`.
- `FixtureDef` consumes texture output through `bindings.input` and still has
  `output_loc` for output sink registration.
- `examples/basic` uses bus-first shader -> texture binding and direct
  texture -> fixture binding.

### Tests

- `ShaderNode`, `FixtureNode`, and `TextureNode` each have local unit tests.
- `CoreProjectRuntime` and `project_loader` tests construct full runtime scenes
  using `TextureNode`.
- This is good news for M2.4: there is already a local validation surface that
  can be updated without any wire/UI work.

## Open Questions

### Q1. What Is The Stepping-Stone Render Product Model For M2.4?

Context: the previous draft assumed `TextureNode` should go away, but the
refined direction is to keep it and give it a real role. The user clarified the
intended render-product split:

- on-demand render products: nothing rendered yet; can render when asked
- texture-backed render products: producer-owned texture already exists

Suggested answer: M2.4 should support one render-product flavor only:
on-demand, full-texture rendering. In this model, `ShaderNode` produces a
render product capability, `TextureNode` owns the texture target, and the
texture node asks the shader product to render into the full texture.

### Q2. What Should The Truthful Runtime Flow Be In M2.4?

Context: M2.3 should already have moved authored connectivity onto source-side
slot bindings, so M2.4 can treat loader wiring as interpretation of those
bindings rather than bespoke `texture_loc` / `output_loc` fields. The new
stepping-stone model is not “shader owns texture” and not yet “fixture consumes
render product directly.”

Suggested answer:

- shader produces render product on its output slot
- texture consumes render product on an input slot and materializes texture
  storage / texture-backed runtime state
- fixture consumes texture output
- output remains downstream of fixture as today

This is a much cleaner stepping stone and minimizes churn.

### Q3. Should `OutputNode` Become A Real Consumer Now?

Context: today `OutputNode` is a passive sink allocator and `FixtureNode`
writes output bytes directly into the output sink buffer. If we are making the
runtime graph truthful, there are two reasonable models:

- fixture remains the node that materializes output-channel data and writes the
  sink buffer directly
- or output becomes a real consumer node that takes fixture-produced channel
  data through bindings

Suggested answer: if one-plan scope is the goal, keep `OutputNode` as a sink
boundary for M2.4 and focus on removing `TextureNode` plus making
shader->texture->fixture truthful. Turning output into a full consumer is appealing,
but it is a second semantic refactor and could easily balloon the plan.

User answer: yes. Outputs are intentionally different IO nodes, not generic
product consumers. Fixtures remain demand roots that drive the graph, and
outputs remain direct-write sink boundaries that flush after tick.

### Q4. Can We Remove All Node Projection Hooks In M2.4?

Context: `fixture_projection_info()` and `shader_projection_wire()` look dead.
`runtime_output_sink_buffer_id()` and `primary_render_product_id()` are still
used by `Engine` and `project_loader`.

Suggested answer:

- remove `fixture_projection_info()` and `shader_projection_wire()` now
- keep the resource-id hooks that are still structurally live
- revisit those remaining hooks after the node-flow refactor, once we know what
  resource ownership API we actually want

### Q5. Does `TextureNode` Stay As A First-Class Runtime Node?

Context: after refinement, the answer appears to be yes.

Suggested answer: yes. `TextureNode` stays, but it changes from a metadata stub
into a real render-target node with an input binding and output dataflow role.


## Dependency On M2.3

M2.4 should start from these assumptions after M2.3:

- source defs carry a shared authored binding container
- produced vs consumed binding direction is explicit in authored TOML
- bus-first bindings are the idiomatic source shape
- direct node-slot refs remain available for explicit local wiring
- the loader can read authored bindings as the source of runtime edge intent

If those assumptions are not true yet, M2.4 scope is premature and should not
quietly absorb them.
