# M2.4 Runtime Node Truth Pass Design

## Scope Of Work

M2.4 refactors the runtime node graph so it reflects the actual domain model we
want before canonical sync/frontend work resumes.

In scope:

- Keep `TextureNode`, but give it a real runtime role.
- Make `ShaderNode` produce a render product on its output slot.
- Make `TextureNode` consume that render product and own the concrete texture
  target/materialization boundary.
- Make `FixtureNode` consume texture output while remaining a demand root.
- Keep `OutputNode` as a special IO sink/flush boundary rather than a generic
  downstream product consumer.
- Remove dead legacy projection hooks from `Node`.
- Update loader wiring, runtime tests, and example artifacts to match the new
  runtime flow.

Out of scope:

- Canonical wire/project sync work.
- Client/view/frontend work.
- Runtime slot root sync exposure.
- Formal runtime taxonomy types for input/middle/output nodes.
- Turning output into a generic consumer node.

## File Structure

```text
lp-core/lpc-engine/src/
  node/
    node.rs
    contexts.rs
    node_error.rs
    pressure_level.rs

  nodes/
    mod.rs
    placeholder/
      mod.rs

    shader/
      mod.rs
      shader_node.rs

    texture/
      mod.rs
      texture_node.rs

    fixture/
      mod.rs
      fixture_node.rs
      gamma.rs
      mapping/
        mod.rs
        accumulation.rs
        entry.rs
        points.rs
        precompute.rs
        structure.rs
        overlap/
        sampling/

    output/
      mod.rs
      output_node.rs

  project_runtime/
    project_loader.rs
    core_project_runtime.rs
    runtime_services.rs
    source_authoring_index.rs
```

## Architecture Summary

M2.4 keeps the authored `TextureDef` / `TextureNode` concept, but stops using
it as a hollow metadata shim.

The truthful runtime flow becomes:

- `ShaderNode` is an on-demand render-product producer.
- `TextureNode` is a real middle node that consumes that render product and
  materializes a concrete texture target it owns.
- `FixtureNode` is a demand root that consumes texture output and performs
  fixture mapping/sampling.
- `OutputNode` remains an IO sink node: fixture writes output-channel data into
  sink buffers and runtime services flush those sinks after tick.

This is a stepping-stone render-product model:

- Support one render-product flavor in M2.4: on-demand, full-texture render.
- A shader does not own the final texture storage.
- The texture node owns the texture target and asks the shader product to fill
  it.
- Future work may add direct render-product consumers or producer-owned
  texture-backed products, but that is not required here.

## Main Components And Interactions

### `Node` Trait

`Node` should return to being a runtime execution/resource contract, not a
legacy sync projection surface.

In M2.4:

- remove `fixture_projection_info()`
- remove `shader_projection_wire()`
- keep `runtime_output_sink_buffer_id()` while output flushing still depends on
  it
- keep `primary_render_product_id()` while render-product ownership remains
  queried through the engine

### `ShaderNode`

`ShaderNode` should:

- compile GLSL through `LpGraphics`
- expose one produced slot for render output, ideally `output`
- behave as an on-demand render-product producer
- stop depending on a fake texture metadata node relationship for its semantic
  role

For the M2.4 stepping-stone, it may still need target dimensions/config routed
through the texture node interaction, but the important change is that its
runtime contract becomes “produce render product,” not “own texture output.”

### `TextureNode`

`TextureNode` becomes the concrete render-target/materialization boundary.

It should:

- consume a shader render product through a binding/consumed slot
- own the actual texture target / backing resource
- trigger full-texture rendering into that owned target
- expose downstream texture output/metadata for fixture consumption

This preserves the authored `texture.toml` shape while making the runtime node
honest.

### `FixtureNode`

`FixtureNode` remains a demand root and always ticks to drive the graph.

It should:

- consume texture output rather than reaching across to both shader and texture
  in an ad hoc way
- keep the existing mapping/gamma/domain logic that was promoted out of
  `legacy/`
- continue writing into lamp-color and output sink buffers

### `OutputNode`

`OutputNode` remains a special IO sink boundary.

It should:

- allocate/own sink buffers used for output flushing
- not become a generic consumer node in M2.4
- remain compatible with `RuntimeServices::flush_dirty_output_sinks`

### `CoreProjectLoader`

`CoreProjectLoader` becomes the central place where authored refs are turned
into the truthful runtime graph.

It should wire:

- shader output -> texture input
- texture output -> fixture input
- fixture -> output sink

This is also where any authored/runtime mismatch around old `texture_loc` or
intermediate lookup logic gets cleaned up.

## Important Constraints

- Keep this milestone local-runtime focused. Do not pull canonical sync design
  into the implementation.
- Update examples last, after tests prove the new runtime shape.
- Prefer deleting stale compatibility surfaces instead of renaming them into
  permanence.
- Keep output semantics explicit: outputs are IO nodes, not generic render/data
  consumers.
