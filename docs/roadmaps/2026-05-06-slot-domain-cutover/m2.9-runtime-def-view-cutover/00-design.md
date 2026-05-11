# M2.9 Runtime Def View Cutover Design

## Scope

Convert concrete runtime nodes toward resolver-backed authored config access.
The milestone applies generated `*DefView` helpers to all current node defs and
uses those views in runtime nodes where doing so is behaviorally clear.

In scope:

- Generate `ShaderDefView`, `FixtureDefView`, and `OutputDefView`.
- Remove direct stored `ShaderDef` from `ShaderNode`.
- Read shader compile options through resolver-backed slot views during tick.
- Move fixture scalar config reads through resolver-backed slot views.
- Keep loader-owned graph/resource setup in the loader.
- Keep output service registration loader-side.

Out of scope:

- Wire/view sync rebuild.
- Client/server mutation messages.
- Dynamic shader param shape changes.
- Resolver-backed fixture mapping aggregates and resource resizing.
- Changing the fixture-to-output runtime buffer flow.

## File Structure

```text
lp-core/lpc-model/src/nodes/
  shader/shader_def.rs
  fixture/fixture_def.rs
  output/output_def.rs
  texture/texture_def.rs

lp-core/lpc-model/build.rs
lp-core/lpc-slot-codegen/src/lib.rs

lp-core/lpc-engine/src/nodes/
  shader/shader_node.rs
  fixture/fixture_node.rs
  output/output_node.rs
  texture/texture_node.rs

lp-core/lpc-engine/src/project_runtime/
  project_loader.rs

docs/roadmaps/2026-05-06-slot-domain-cutover/m2.9-runtime-def-view-cutover/
  00-notes.md
  00-design.md
  01-generated-node-def-views.md
  02-shader-node-def-view.md
  03-fixture-node-def-view.md
  04-output-and-loader-cleanup.md
  05-cleanup-validation.md
  future.md
```

## Architecture Summary

`SlotPath` remains the authored and wire-facing address. Generated `*DefView`
types are runtime helpers that cache compiled `SlotAccessor`s against
`SlotShapeRegistry::revision()`. Runtime nodes use views through `TickContext`,
which resolves bindings first and falls back to the authored `NodeDef` stored in
`ArtifactStore` via the node's `NodeDefHandle`.

The loader still owns graph construction and setup that cannot happen through a
runtime resolver session:

- load shader GLSL source from `glsl_path`;
- attach runtime nodes;
- register authored bindings;
- resolve fixture output sink ids;
- register output service sinks.

`ShaderNode` keeps the GLSL source text and runtime compiler state, but no
longer stores the whole `ShaderDef`. On tick, it reads compile-option slots
through `ShaderDefView` and caches the compact model options needed by render.
Render remains resolver-free.

`FixtureNode` keeps mapping and output sink data for now because mapping is an
aggregate and resource allocation currently depends on it. Scalar config reads
move to `FixtureDefView` so bindings can override authored defaults on the same
resolver path.

`OutputNode` remains a minimal sink node. `OutputDefView` is generated and
tested as evidence, but the runtime node does not read output config until the
output service boundary is revisited.

## Main Interactions

1. Loader loads `NodeDef` artifacts and attaches runtime nodes.
2. Loader registers authored bindings on the `NodeTree`.
3. During tick, runtime nodes call generated `*DefView::get_or_compile`.
4. Runtime nodes pass field accessors to `TickContext`.
5. The resolver checks bindings by semantic path and falls back to authored
   `NodeDef` data through the accessor.
6. Runtime nodes update their runtime state roots from resolved config and
   produced values.
