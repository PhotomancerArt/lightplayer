# M2.9 Runtime Def View Cutover Notes

## Scope

Convert the concrete runtime nodes so authored config is read through generated,
resolver-backed slot views instead of through copied `*Def` structs or
constructor-expanded config fields.

This plan should:

- Extend generated `*DefView` coverage beyond `TextureDef`.
- Use `TickContext::resolve_consumed_slot_accessor_value` for runtime config reads.
- Keep bindings and authored defaults on one resolver path.
- Reduce duplicated config state stored inside runtime nodes.
- Preserve the current render flow and example behavior.

This plan should not:

- Rebuild wire/view sync.
- Implement client/server mutation messages.
- Implement dynamic shader param shape changes.
- Redesign fixture-to-output data flow.
- Remove texture node unless a narrow cleanup becomes obviously safe.

## Current State

### Slot Views And Accessors

- `lpc-model` has `SlotAccessor`, compiled against `SlotShapeRegistry::revision()`.
- `TickContext::resolve_consumed_slot_accessor_value` resolves a consumed slot by compiled accessor.
- `lpc-slot-codegen` generates `*View` types for records marked `#[slot(root, view)]`.
- `TextureDef` is currently `#[slot(root, view)]`; `ShaderDef`, `FixtureDef`, and `OutputDef` are only `#[slot(root)]`.
- Generated views currently expose `&SlotAccessor` methods for every non-skipped root field.
- Generated views compile field paths with `SlotAccessor::compile`, not `compile_value`, so aggregate fields like maps/options/enums can have accessors but cannot necessarily be read as typed values with the current `TickContext` helper.

### Engine Defaults And Bindings

- `NodeEntry` stores a `NodeDefHandle`.
- Unbound consumed slots fall back to the authored `NodeDef` in `ArtifactStore`.
- Runtime bindings are now stored on `NodeTree`; resolver asks the active host for bindings.
- For a consumed slot, binding lookup still matches by semantic `SlotPath`; the authored fallback can use compiled accessors.

### Current Nodes

- `TextureNode` is already close to the desired shape:
  - owns only runtime state and `Option<TextureDefView>`.
  - reads `size` with `ctx.resolve_consumed_slot_accessor_value(self.def_view(ctx)?.size())`.
- `ShaderNode` still owns a cloned `ShaderDef`:
  - uses `config.glsl_opts` during compile.
  - receives `glsl_source` from the loader.
  - `tick()` only refreshes output state and does not read authored config.
- `FixtureNode` still receives many extracted config values in `new()`:
  - `render_width`, `render_height`, `mapping`, `mapping_version`, `color_order`, `brightness`, `gamma_correction`.
  - tick uses the copied config plus `input` binding.
  - `init_resources` allocates its lamp-color buffer from copied mapping before the first tick.
- `OutputNode` currently has no config view:
  - loader reads `OutputDef` and registers the output sink in `RuntimeServices`.
  - runtime node only owns a channel buffer id.

### Loader

- `CoreProjectLoader::attach_loaded_nodes` still reads concrete `NodeDef` values to:
  - resolve shader GLSL path and read source text before creating `ShaderNode`.
  - register shader target bindings.
  - resolve fixture output node locator and output sink before creating `FixtureNode`.
  - pass fixture config fields into `FixtureNode::new`.
  - register output sink services from `OutputDef`.
- Loader should still be allowed to use authored defs for graph construction and non-runtime setup where there is no resolver session yet.
- Runtime nodes should avoid storing copied config when the value can reasonably be read through the resolver during tick/render.

## User Direction

- This is big enough for a full plan.
- The goal is getting all concrete nodes converted to the new view work.
- We are in a prune/optimize/fix phase: remove speculative or duplicated machinery when it is not needed.
- Code size matters on ESP32, so avoid adding broad abstractions that are not used right away.

## Open Questions

### Q1: How far should this milestone push loader ownership?

Context:

- Shader source loading currently happens in the loader because `glsl_path` is a source-file path relative to the shader artifact.
- Fixture output sink lookup currently happens in the loader because it needs the output node's runtime buffer id.
- Output sink registration currently happens in the loader because `RuntimeServices` owns output flushing.

Suggested answer:

- Keep loader-owned graph/resource setup in this milestone.
- Move runtime-tick/render config reads onto views.
- Do not try to make shader GLSL source path dynamically bound yet; source reload/mutation can revisit this.

User answer:

- Accepted.

### Q2: Should generated views be enabled for all node defs immediately?

Context:

- Marking `ShaderDef`, `FixtureDef`, and `OutputDef` with `#[slot(root, view)]` should generate views with root-field accessors.
- Some fields are aggregates (`bindings`, `mapping`, `param_defs`, `options`) that generated views can expose as accessors, but current typed value reading is mostly for value leaves.

Suggested answer:

- Enable generated views for all concrete node defs now.
- Use only the fields that the runtime can read safely today.
- Leave aggregate typed view ergonomics for a later pass unless fixture mapping requires it.

User answer:

- Accepted.

### Q3: How should `FixtureNode` handle mapping and resource allocation?

Context:

- Fixture mapping is a large aggregate enum/record, not a simple value leaf.
- `FixtureNode::init_resources` currently needs mapping before tick to allocate the lamp color buffer.
- If mapping is resolver-backed, the first tick may be the first time the node can resolve it.

Suggested answer:

- Do a pragmatic slice:
  - keep mapping copied in `FixtureNode` for this milestone, because aggregate slot reads and buffer resizing need a cleaner design.
  - move scalar/value config (`render_size`, `color_order`, `brightness`, `gamma_correction`, possibly `transform`) through `FixtureDefView`.
  - add future notes for resolver-backed aggregate mapping and resource resizing.

User answer:

- Accepted.

### Q4: Should `OutputNode` read `OutputDef` through a view?

Context:

- `OutputNode` runtime currently does not use `pin` or options; `RuntimeServices` uses output def data to flush sinks.
- Moving `OutputDef` reads into the node could fight the current output-service boundary.

Suggested answer:

- Generate `OutputDefView` and add tests that it compiles.
- Do not force `OutputNode` to read config it does not semantically use.
- Keep output service registration loader-side until output flushing is reworked.

User answer:

- Accepted.

### Q5: Should `ShaderNode` compile options be resolver-backed?

Context:

- `ShaderNode::ensure_compiled` runs during render, using `RenderContext`, not `TickContext`.
- `RenderContext` currently has graphics, node id, revision, and time, but no resolver/session access.
- `ShaderNode` currently stores `ShaderDef` only to access `glsl_opts`.

Suggested answer:

- Avoid giving `RenderContext` resolver access in this milestone.
- Move/refresh shader compile options during `tick()` using `ShaderDefView`, store the computed `ShaderCompileOptions` or model `GlslOpts` in `ShaderNode`, and have `ensure_compiled` use that cached runtime copy.
- Mark shader recompilation invalidation on config change as a follow-up if current machinery cannot cheaply detect it.

User answer:

- Accepted.

### Q6: What should happen to old convenience methods on defs?

Context:

- `ShaderDef::glsl_path_buf`, `FixtureDef::render_width`, `FixtureDef::brightness_u8`, etc. encourage direct def reads.
- Some are still useful for loader setup and tests.

Suggested answer:

- Do not delete all convenience methods immediately.
- Stop using them in runtime nodes where a resolver-backed view is available.
- After node conversion, audit which methods remain loader/test-only and prune or document accordingly.

User answer:

- Accepted.

## Suggested Milestone Goal

By the end of M2.9, the runtime node constructors should be noticeably thinner:

- `TextureNode::new(node_id)` remains view-backed.
- `ShaderNode::new(node_id, glsl_source)` no longer stores `ShaderDef`.
- `FixtureNode::new(...)` should stop taking scalar config values; it may still take mapping/output sink until aggregate config and output flow are addressed.
- `OutputNode` remains minimal, with `OutputDefView` generated and available for later output-service work.

Validation should prove:

- Generated views compile for all concrete node defs.
- Shader compile options can be read through resolver-backed authored defaults.
- Fixture scalar config changes through bindings affect runtime behavior.
- Existing project loading and basic examples still work.
