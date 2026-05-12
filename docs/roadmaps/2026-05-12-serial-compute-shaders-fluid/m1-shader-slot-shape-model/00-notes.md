# M1 Notes: Shader Slot Shape Model

## Scope

This plan covers the first milestone of the serial compute shader + fluid roadmap:
define the authored shader slot shape model needed before compute shaders can run.

In scope:

- Add a general shader slot definition vocabulary for authored shader inputs and outputs.
- Represent compute shader consumed and produced slots in TOML.
- Support value shapes needed by the first fluid emitter example: scalars, vectors, structs, and bounded/fixed homogeneous sequences.
- Add semantic `FluidEmitter` / `FluidEmitterSet` value shapes in `lpc-model`.
- Produce deterministic shader header text from authored slot definitions as evidence that TOML can be the source of truth.
- Add round-trip and evidence tests in `lpc-model`.

Out of scope:

- Executing compute shaders.
- Adding a runtime compute shader node.
- Adding a fluid node runtime.
- Parsing shader source annotations.
- Building a UI header rewrite workflow.

## User Notes

- The purpose is to build compute shaders first, then use a fluid node to prove they are useful.
- TOML is the source of truth for shader slot shape.
- UI can later generate or update a bounded shader header region, probably `// gen:header` / `// gen:header:end`.
- First compute shaders are serial: the shader runs once per tick/frame, not over a GPU-style workgroup.
- Slots map cleanly onto uniforms or normal globals in the shader ABI for this first serial version.
- Compute shader outputs are normal produced slot values for now; do not invent a `ComputeProduct` in M1.
- The first useful shape is something like an emitter list:

  ```glsl
  struct Emitter {
      vec2 pos;
      vec2 dir;
      // ...
  };

  out Emitter[4] emitters;
  in float time;
  ```

- Emitter/touch-like data exposes a deeper unresolved domain problem around
  collection semantics:
  - static vs dynamic list sizing;
  - merging values from multiple producers;
  - stable identity for entries.
- The natural authoring/runtime shape for many of these is probably
  `Map<id, thing>`, not a plain list.
- Multiple emitter generators should eventually be able to bind to a shared
  destination and merge by id.
- M1 should acknowledge this pressure, but should not try to solve generalized
  merge semantics yet.

## Current Codebase State

### Slot And Value System

- `lpc-model` already has the slot/value split this milestone needs.
- `LpValue` is the portable disk/wire value payload in `lp-core/lpc-model/src/value/lp_value.rs`.
- `LpType` is the structural storage type in `lp-core/lpc-model/src/value/lp_type.rs`.
- `LpType` already supports scalar/vector/matrix values plus:
  - `Array(Box<LpType>, usize)` for fixed-size homogeneous sequences;
  - `List(Box<LpType>)` for variable-length homogeneous sequences;
  - `Struct { name, fields }`.
- `SlotValue` and `SlotValueShape` in `lp-core/lpc-model/src/slot/slot_value.rs` already express the boundary where the slot tree stops and one complete `LpValue` begins.
- Semantic value leaves already live in `lp-core/lpc-model/src/slots/`, with examples such as `dim2u.rs`, `xy.rs`, `u32_list.rs`, `visual_product.rs`, and `control_product.rs`.
- `ValueSlot<T>`, `MapSlot<K, V>`, and `OptionSlot<T>` in `lp-core/lpc-model/src/slot/value_slot.rs` provide revision-tracked Rust-authored containers.

### Existing Shader Defs

- `ShaderDef` lives in `lp-core/lpc-model/src/nodes/shader/shader_def.rs`.
- It currently contains:
  - `glsl_path: SourcePathSlot`;
  - `render_order: RenderOrderSlot`;
  - `bindings: BindingDefs`;
  - `glsl_opts: GlslOpts`;
  - `param_defs: MapSlot<String, ShaderParamDef>`.
- `ShaderParamDef` in `lp-core/lpc-model/src/nodes/shader/shader_param_def.rs` is still a narrow visual-param model:
  - label/description;
  - `value_type: ValueSlot<String>`;
  - `default: RatioSlot`;
  - optional scalar min hint.
- That is too stringly typed and scalar-specific for compute shader inputs and outputs.

### Node Def Parsing

- `NodeDef` lives in `lp-core/lpc-model/src/nodes/node_def.rs` and is the canonical closed enum for authored node defs.
- Current variants are `Project`, `Texture`, `Shader`, `Output`, and `Fixture`.
- `NodeDef::from_toml_str` probes the top-level `kind` string and dispatches to the concrete def type.
- `NodeKind` in `lp-core/lpc-model/src/node/kind.rs` mirrors the same closed set.
- Adding a first-class compute shader artifact means touching these central switch points.

### Mockup Lessons

- `lpc-slot-mockup` has runtime materialization pressure around shader params.
- `lp-core/lpc-slot-mockup/src/engine/shader_node.rs` turns authored `ShaderDef.param_defs` into a dynamic runtime `SlotRecord`.
- The mockup derives runtime param field shapes from the actual default `LpValue`; M1 should instead make the authored type explicit so empty arrays/lists and outputs have real shapes.

### Fluid Spike

- Existing fluid investigation lives under `lp-fw/fw-esp32/src/tests/`.
- `msafluid_solver.rs` contains the Q32 RGB solver.
- `fluid_demo/emitters.rs` contains a `FluidPulser` and emitter routines with fields/concepts that inform `FluidEmitter`:
  - normalized position;
  - direction or target-derived angle;
  - radius;
  - RGB dye;
  - velocity;
  - intensity.
- The old spike uses `f32` at emitter configuration boundaries and Q32 in solver internals. M1 is only defining portable values, not optimizing the runtime ABI.

## Initial Direction

- Keep `LpType` / `LpValue` as the structural transport grammar.
- Add a shader-specific authored slot definition that points to an explicit `LpType` or semantic `SlotValueShape`.
- Do not reuse `ShaderParamDef` as-is; it is too narrow and its `value_type: String` is the wrong direction.
- Treat compute shader slot definitions as a node-def authored language, not a runtime resolver feature yet.
- Add `FluidEmitter` / `FluidEmitterSet` as semantic slot values using `LpValue::Struct` and `LpValue::Array`.
- Add a small header generator in `lpc-model` so tests can prove the TOML slot definitions can produce GLSL declarations.
- Keep the current `LpValue` name for M1, but record that it likely wants a
  future rename to `LpsValue` or a similar LightPlayer-system value name.
- M1 should establish map semantics at the shader-slot model layer, even if
  runtime resolver merge behavior waits.
- Non-leaf bindings are required by the direction of the model. A binding to a
  whole map slot must be valid in the future.
- Shader slot defs should be able to reference native LightPlayer value shapes
  by an internal type name rather than reproducing their fields. For M1, use an
  explicit native name style like `lp::fluid::Emitter`. This is not a complete namespacing
  system yet, but it makes the authored intent explicit.

## Open Questions

### Q1: Separate `ComputeShaderDef` Or `ShaderDef` Mode?

Context:

- Current `ShaderDef` means visual shader node and carries `render_order`.
- Serial compute shader runtime semantics will differ: no visual product, produced slot values, likely no render order.
- `NodeDef` and `NodeKind` are already closed enums.

Suggested answer:

- Add a separate `ComputeShaderDef` with `kind = "shader/compute"`.
- Keep it under `nodes/shader/` because it is part of the shader family, but make the type first-class.
- Add `NodeDef::ComputeShader` and `NodeKind::ComputeShader`.

Answer:

- Yes. Use a separate `ComputeShaderDef` with `kind = "shader/compute"`.

### Q2: Replace `ShaderParamDef` Now Or Add A New General Slot Def?

Context:

- `ShaderParamDef` is currently only used by model tests and the mockup, not the real engine shader runtime.
- It is scalar-ish and stringly typed.
- Visual shader params will eventually want the same general typed slot language, but M1 does not need to migrate the visual shader runtime.

Suggested answer:

- Add a new `ShaderSlotDef` / `ShaderSlotDefs` vocabulary and use it for compute shader inputs/outputs.
- Leave `ShaderParamDef` in place for now, but mark the relationship clearly in docs/comments.
- Later migrate visual shader params to the same authored slot model.

Answer:

- Replace it rather than duplicating the concept.
- `ShaderParamDef` is not exercised by canonical examples or the real runtime path; it mostly exists in model tests and the mockup.
- Add `ShaderSlotDef` as the general authored shader ABI slot definition and migrate `ShaderDef.param_defs` to use it.
- Delete `ShaderParamDef` / `ScalarHint` from `lpc-model` unless implementation uncovers a real dependency worth preserving.
- Update mockup/tests that still use the old name only as needed to keep the workspace coherent.

Follow-up context:

- A code search found real-code references in `lpc-model`, project-builder defaults, `lp-cli` defaults, and `lpc-wire` tests, but no canonical example use and no real engine runtime use.
- `lpc-slot-mockup` has many old param-def references because it was the pressure harness for this model.
- Code size matters; avoid keeping both param and slot vocabularies around just for transition comfort.

### Q3: How Should `FluidEmitterSet` Represent Count?

Context:

- `LpType` supports both `Array(T, N)` and `List(T)`.
- Runtime wants bounded data for embedded memory predictability.
- Authored shader output needs an ABI count for header generation, e.g. `Emitter[4] emitters`.
- UI/wire may prefer a variable logical count.

Suggested answer:

- In M1, make the shader slot definition carry a fixed or bounded sequence count for ABI/header generation.
- Represent the actual `FluidEmitterSet` payload as an `LpValue::Array` of emitters.
- Validate length against the authored slot definition in tests, but defer a full generic validation API until runtime integration needs it.

Additional pressure:

- Plain list/array representation is probably not the long-term semantic model
  for emitters, touches, or similar data.
- The domain likely wants stable-key maps so multiple producers can merge by id
  and clients can diff/update entries cleanly.
- GLSL still wants bounded/fixed storage, so a future bridge probably maps a
  stable-key map to a bounded ABI array plus count.
- A fluid node's natural consumed slot shape is likely
  `MapSlot<u32, Emitter>`, giving per-emitter addressing and per-emitter
  revisions.
- A shader cannot directly produce a map in GLSL. Its natural ABI shape is
  something like `out Emitter emitters[4]; out uint emitter_count;`, or an
  array with an id sentinel.
- This implies a first-class conversion layer between shader ABI values and
  semantic slot data, not an ad hoc special case in the resolver.

Answer:

- The semantic shape should be map-like. For fluid emitters, the natural
  consumed slot is `MapSlot<u32, Emitter>`.
- The shader owns the conversion between semantic slot shape and GLSL ABI shape.
- A map should be a first-class `ShaderSlotDef` shape, with authored GLSL mapping
  details.
- For M1, restrict shader map keys to `u32` unless implementation reveals a
  compelling reason to include `i32`.
- The first map-to-GLSL strategy can use a fixed/bounded array with a sentinel
  id. `0` is attractive because it is the default value and can mean "unused".
- A count side-channel remains plausible, but sentinel-id is a simple first
  strategy.
- The resolver must eventually handle bindings at aggregate slot boundaries,
  such as binding a whole `SlotMap`, not only binding one leaf value. This is a
  downstream engine/resolver requirement, not necessarily M1 implementation.
- Merge strategy belongs on the receiving/consumed slot, not on produced slots
  and probably not on individual bindings. The conflict exists because multiple
  bindings converge on one target, and per-binding merge policies could
  disagree.
- Use `merge` as the authored name. Likely values: `by_key`, `latest`, and
  `error`.
- A produced shader slot owns ABI mapping only; a receiving slot owns how
  multiple incoming values are combined.
- Use `mapping = { kind = "sentinel", ... }` for shader slot ABI mapping in M1.
  This keeps the TOML compact while leaving room for future mapping strategies.
- For M1/M2 a basic replace/overwrite strategy is enough, but data merging is a
  first-class future concept.
- Slot probes/explain output will eventually need to report merge behavior and
  conflict handling, not only "where the value came from".

Planning implications:

- Do not model `FluidEmitterSet` as merely "an array of emitters".
- Model the semantic shape as map-like data with `u32` keys whose values
  reference the native `lp::fluid::Emitter` shape.
- Model the GLSL ABI as a bounded array strategy owned by the shader slot.
- Keep merge policy vocabulary attached to receiving/consumed slots in notes and
  types if cheap, but do not implement resolver merge execution in M1.
- Ensure the plan explicitly calls out follow-up work for aggregate/non-leaf
  bindings and merge explanation.

### Q6: Does M1 Need A `FluidEmitterSet` Type?

Context:

- The natural semantic collection is `MapSlot<u32, FluidEmitter>`.
- A wrapper set type would duplicate what the slot map already expresses unless
  it owns distinct set-level behavior or metadata.

Answer:

- No, not for M1.
- Add `FluidEmitter` as the native value shape.
- Represent an emitter set as a map slot shape with `u32` keys and
  `lp::fluid::Emitter` values.

### Q7: How Should Native Shape Names Work?

Context:

- `SlotShapeId` is currently a compact `u32` hash generated from arbitrary
  strings via `SlotShapeId::from_static_name`.
- `#[derive(SlotRecord)]` defaults static root names to
  `module_path!()::TypeName`.
- `StaticSlotShape::shape_name()` and `SlotShapeRegistry::ensure_root_named`
  already support a human-readable name alongside the compact id.
- Shader authored TOML needs a clear way to reference native LightPlayer shapes
  without copying their structure.

Answer:

- Adopt explicit `lp::<domain>::<RustName>` names as the M1 native shape
  reference style, e.g. `lp::fluid::Emitter`.
- Ensure the static shape registration path can register this name for
  `FluidEmitter`.
- Ensure shader slot type references can resolve or at least validate against
  the shape registry by this native name.
- Keep this as a small native-name convention, not a full namespace system.

### Q4: Where Should Fluid Semantic Values Live?

Context:

- General semantic leaves live in `lpc-model/src/slots/`.
- Node/domain-specific defs live in `lpc-model/src/nodes/<domain>/`.
- `FluidEmitter` is not a generic UI primitive like `Dim2u`; it is domain data for the future fluid node and compute shader example.

Suggested answer:

- Add `lp-core/lpc-model/src/nodes/fluid/` with `fluid_emitter.rs` and `fluid_emitter_set.rs`.
- Re-export from `nodes::fluid`.
- If it later becomes broadly useful outside the fluid domain, promote or alias it from `slots/`.

Answer:

- Yes. Put fluid semantic values under `lpc-model/src/nodes/fluid/`.

### Q5: Header Generator Location And Strictness?

Context:

- M1 needs evidence that TOML can drive shader header text.
- Full GLSL parsing/annotation editing is out of scope.
- `lpc-model` already owns the domain model and can generate deterministic text without engine runtime knowledge.

Suggested answer:

- Add a small generator under `lpc-model/src/nodes/shader/shader_header_gen.rs`.
- Generate only declarations for the supported M1 type subset.
- Return clear errors for unsupported `LpType` forms rather than silently guessing.
- Tests should snapshot/assert important header snippets rather than requiring production-perfect formatting.

Answer:

- Yes. Put the header generator under `lpc-model/src/nodes/shader/shader_header_gen.rs`.
