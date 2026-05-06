# M2 Source Def Slot Roots Notes

## Scope Of Work

Milestone 2 exposes real TOML-authored source node definitions as slot roots.

In scope:

- Add production slot-root access for real `lpc-source` node definitions:
  - `ProjectDef`
  - `NodeInvocation`
  - `TextureDef`
  - `ShaderDef`
  - `OutputDef`
  - `FixtureDef`
- Register source shapes in a production shape registry.
- Prove generic traversal and snapshotting over actual source defs loaded from TOML, starting with `examples/basic`.
- Decide whether shader `param_defs` enter the real source model in this milestone.
- Keep the source model `no_std + alloc`.

Out of scope:

- Runtime node state, params, and output roots.
- Replacing project wire sync.
- Client-driven source/artifact mutation.
- Generic UI changes beyond tests/helpers needed to verify source roots.
- Removing legacy detail projection.

## User Notes And Decisions

- M2 follows the slot domain cutover roadmap at `docs/roadmaps/2026-05-06-slot-domain-cutover/`.
- M1 established runtime vocabulary: slots use `SlotPath`, map keys use bracket syntax, and legacy detail concepts should remain explicitly named as legacy until deleted.
- User wants aggressive renaming and cleanup during this domain-definition push.
- User prefers positive boolean metadata names such as `writable`.
- User wants record keys validated as real identifiers. `SlotName` now validates ASCII identifiers; arbitrary strings belong in map keys.
- `examples/basic` is the canonical fixture for early source work.
- Backwards compatibility with old examples is not important, but avoid broad example churn early.
- `project.toml` and node TOML files are the authored source of truth; directory discovery is gone.

## Current Codebase State

### M1.2 Outcome

- `ValueSlot<T>`, `MapSlot<K,V>`, and `OptionSlot<T>` now serialize as clean
  authored values and deserialize with `current_state_version()`.
- All current semantic slots under `lpc-model/src/slot/slots/` support authored
  serde.
- `MapSlot<String, V>`, `MapSlot<u32, V>`, and `MapSlot<i32, V>` round-trip
  through authored map/table keys. TOML table keys are strings at the serde
  boundary, so numeric keys are parsed through `MapSlotKeyLike`.
- `lpc-slot-mockup` now proves the target shape with source-like defs,
  generated TOML evidence, shader `param_defs`, and fixture `path_points`
  backed by stable-key maps.

### Slot Model

- `lpc-model/src/slot` contains the production slot primitives:
  - `SlotAccess`
  - `StaticSlotAccess`
  - `SlotDataAccess`
  - `SlotRecordAccess`
  - `MapSlotAccess`
  - `SlotEnumAccess`
  - `OptionSlotAccess`
  - `SlotRecordShape`
  - `SlotEnumShape`
  - `SlotShapeRegistry`
  - `SlotPath`
- `SlotPath` now distinguishes:
  - `SlotPathSegment::Field(SlotName)` for records/enums/options.
  - `SlotPathSegment::Key(MapSlotKey)` for maps.
- `SlotName` is the record/enum/option field-token type and is now identifier-like:
  - first char: ASCII alpha or `_`
  - later chars: ASCII alpha, digit, or `_`
- `SlotData` is the owned snapshot/wire mirror. Rust-authored structs can expose access traits without first converting themselves into `SlotData`.
- `ValueSlot<T>`, `MapSlot<K,V>`, and `OptionSlot<T>` exist for typed,
  versioned Rust-authored data.
- `lpc-wire/src/slot/access_sync.rs` can snapshot/diff borrowed `SlotAccess` roots through a `SlotShapeRegistry`.

### Slot Derive

- `lpc-slot-macros` provides `#[derive(lpc_model::SlotRecord)]`.
- The derive currently assumes field storage already implements slot access:
  - value/leaf fields are `ValueSlot<T>`-like and expose `ValueSlotAccess`.
  - maps are `MapSlot<K,V>`.
  - options are `OptionSlot<T>`.
  - nested records implement `SlotRecordShape + SlotRecordAccess`.
  - enums implement `SlotEnumShape + SlotEnumAccess`.
- The derive supports:
  - `#[slot(shape_id = "...")]`
  - `#[slot(value = ModelType::...)]`
  - `#[slot(leaf = some_shape())]`
  - `#[slot(record)]`
  - `#[slot(enum)]`
  - `#[slot(map(key = "...", value_ref = "..."))]`
  - `#[slot(option_ref = "...")]`
  - `#[slot(skip)]`
- This works well in `lpc-slot-mockup`, where source structs were authored
  around `ValueSlot` wrappers.

### Real Source Defs

- `lpc-source` is `#![no_std]`.
- Real node defs are plain serde domain structs/enums, not slot-wrapper structs.
- `ProjectDef`:
  - fields: `kind: String`, `name: Option<String>`, `nodes: BTreeMap<NodeName, NodeInvocation>`
  - TOML uses `[nodes.output] artifact = "./output.toml"`.
- `NodeInvocation`:
  - fields: `artifact: ArtifactLocator`, `overrides: Vec<(ValuePath, SrcBinding)>`
  - overrides are transitional and still use `ValuePath` because resolver/binding code has not moved fully to slots.
- `TextureDef`:
  - fields: `width: u32`, `height: u32`
  - mockup used one semantic `Dim2uSlot` field named `size`, but the real TOML currently has flat `width` / `height`.
- `ShaderDef`:
  - fields: `glsl_path: LpPathBuf`, `texture_loc: RelativeNodeRef`, `render_order: i32`, `glsl_opts: GlslOpts`
  - no real `param_defs` yet.
- `OutputDef`:
  - enum with `GpioStrip { pin: u32, options: Option<OutputDriverOptionsConfig> }`
  - `OutputDriverOptionsConfig` has `lum_power`, `white_point`, `brightness`, `interpolation_enabled`, `dithering_enabled`, `lut_enabled`.
- `FixtureDef`:
  - fields: `output_loc`, `texture_loc`, `mapping`, `color_order`, `transform`, `brightness`, `gamma_correction`
  - `mapping` is currently `MappingConfig::PathPoints { paths: Vec<PathSpec>, sample_diameter }`
  - `PathSpec::RingArray` contains `ring_lamp_counts: Vec<u32>`, which does not fit the current slot map-first aggregate vocabulary cleanly.

### Current Basic Example Shape

- `examples/basic/project.toml` has `kind = "project"`, `uid = "basic"`,
  `name = "basic"`, and `[nodes.<name>] artifact = "./node.toml"` tables.
  `ProjectDef` currently has no `uid` field.
- `examples/basic/shader.toml` uses `glsl_path = "shader.glsl"`,
  `texture_loc = "..texture"`, `render_order = 0`, and `[glsl_opts]`.
- `examples/basic/texture.toml` uses flat `width` and `height`.
- `examples/basic/output.toml` uses flat `pin` plus `[options]`.
- `examples/basic/fixture.toml` still uses externally tagged mapping tables and
  arrays:
  - `[mapping.PathPoints]`
  - `[[mapping.PathPoints.paths]]`
  - `[mapping.PathPoints.paths.RingArray]`
  - `ring_lamp_counts = [ ... ]`

M2 should expect to change the fixture TOML shape if we choose the stable-key
map model for real source.

### Real Loading Path

- `lpc-engine/src/project_runtime/project_loader.rs` loads:
  - root `/project.toml`
  - children from `ProjectDef.nodes`
  - typed node defs from each child artifact
- Loaded child defs are stored as `LoadedNodeConfig` and cloned into legacy compatibility projection.
- The project root config on the runtime tree is currently a `NodeInvocation`, not the full `ProjectDef`.
- M2 can stay on source-side tests first and does not need to change project runtime behavior unless we decide to add a registry helper there.

### Mockup Reference

- `lpc-slot-mockup/src/source` already shows the intended slot-shaped source vocabulary:
  - `source.project`
  - `source.node_invocation`
  - `source.shader`
  - `source.shader_param_def`
  - `source.fixture`
  - `source.output`
  - `source.texture`
- The mockup source structs use typed slot wrappers and derive `SlotRecord`.
- Useful ideas to promote:
  - `ProjectDef.nodes` as `MapSlot<String, NodeInvocationDef>`
  - `ShaderDef.param_defs` as `MapSlot<String, ShaderParamDef>`
  - source roots register through `StaticSlotAccess`
  - generic server/client tree walk tests over source roots
- Differences from real source:
  - mockup output is simplified and does not match real `GpioStrip/options`.
  - mockup texture uses `size`, real texture TOML uses `width`/`height`.
  - mockup fixture mapping is simplified, real fixture mapping uses vectors.

## Key Implementation Tensions

### Versioning Authored Source Structs

Real source defs are currently plain deserialized values. Slot snapshots require versions on leaves and structural containers.

Options:

1. Convert source defs to store `ValueSlot<T>` / `MapSlot<K,V>` directly.
   - Pros: one source of truth; works with current derive and access traits; disk, wire, metadata, mutation, and UI all hang off one domain model.
   - Cons: bigger serde/domain churn; runtime code must use `.value()` or explicit mutation APIs because fields are no longer plain values.
2. Add borrowed source adapters that expose a plain def plus an artifact/content frame through `SlotAccess`.
   - Pros: preserves clean source structs and TOML serde.
   - Cons: permanent translation layer; every field needs serde plus adapter shape plus adapter access.
3. Add a snapshot-oriented access layer for immutable source data.
   - Pros: source defs can remain plain and produce `SlotData` snapshots with a supplied frame.
   - Cons: introduces a second access path unless carefully aligned with `SlotAccess`.

Decision: use option 1. Source defs should become authored domain objects whose fields are slot-aware. This matches the desired architecture: the core domain model is the source of truth and carries shape, metadata, versioning, serialization, and mutation semantics. A plain Rust object can still exist inside an atomic `ValueSlot<T>` when the whole object is one lifecycle/version unit.

Implementation implication: M1.2 proved typed slot wrappers can serialize to
authored TOML as their inner values, while `SlotData` remains the generic
wire/snapshot representation with explicit versions.

### Shape Versus TOML Shape

Some slot shapes should be more semantic than current TOML fields:

- `TextureDef` likely wants a `size: Dim2u` slot, while TOML currently has `width` and `height`.
- `OutputDef` is an enum-like root (`GpioStrip`) with nested options.
- `FixtureDef.mapping` uses vectors today; slots currently prefer maps and records, not arrays.

Suggested direction: M2 should expose the existing TOML shape first unless a semantic shape is low-risk and does not obscure the source-to-slot mapping. Record future semantic reshaping rather than hiding real fields.

### Shader Param Defs

The roadmap says to add shader `param_defs` if this is the right time. The mockup proves the shape, but the real source shader TOML currently has no `param_defs`.

Suggested direction: include the type definitions and shape/access for `ShaderParamDef`, but make the TOML field optional/default-empty so `examples/basic` does not need to change unless we want an explicit param fixture.

### Mapping Collections

The current slot model intentionally avoided arrays for versioned slot containers, but `MappingConfig` and `PathSpec` currently contain `Vec`.

Options:

1. Treat mapping as one opaque `ModelValue` leaf for M2.
2. Add a source-specific map projection for vectors using string/u32 keys.
3. Add array support to `SlotShape`/`SlotData` now.

Decision: do not add arrays and do not add custom serde that hides arrays behind maps. Either keep `MappingConfig` as one opaque `ValueSlot<MappingConfig>` for the first source cutover or model paths as `MapSlot<u32, PathSpec>` directly in the authored domain. The preferred direction is `MapSlot<u32, PathSpec>` because stable ids on paths are reasonable, there are no external users yet, and the slot-domain rule against arrays should be applied consistently.

Implication: `examples/basic` can change from TOML arrays to keyed path tables if M2 chooses the structured mapping path.

### Artifact Locator And Path Leaves

`ArtifactLocator`, `LpPathBuf`, and `RelativeNodeRef` are authored string-like values with different semantics.

Suggested direction: add or reuse semantic leaf shapes/conversions:

- `RelativeNodeRef` -> `relative_node_ref_shape()`
- `LpPathBuf` / GLSL path -> `source_path_shape()` or a more precise
  `LpPathBuf`-backed slot if we decide source defs should retain that concrete
  type.
- `ArtifactLocator` -> `artifact_path_shape()` or a more specific artifact
  locator field/access implementation in `lpc-source`.

## Open Questions

### Q1. Should M2 keep real source defs plain and use source adapters, rather than converting fields to `ValueSlot<T>`?

Context: real source defs are clean serde structs today. The current derive assumes versioned wrapper fields, which would churn TOML types and may conflate authored data with runtime/version storage.

Answer: no. Convert real source defs to slot-aware fields. These are authored domain objects, not plain data objects. The ergonomics should reflect the reality that reads and writes carry versioning, metadata, and mutation semantics.

### Q2. Should source def roots expose the `kind` TOML discriminator as a slot field?

Context: every artifact has `kind`, but it is mostly a loader discriminator, not user-editable config. The mockup source roots omit `kind`.

Suggested answer: omit `kind` from editable/display slot roots for concrete node defs. The root shape id and node kind already identify the type. Keep `ProjectDef.name` and child `nodes`.

### Q3. Should `ShaderDef.param_defs` be added in M2?

Context: shader params are one of the reasons source slot roots exist, but current real shader TOML and `examples/basic` do not have param defs. The mockup has `MapSlot<String, ShaderParamDef>`.

Suggested answer: add the field as `#[serde(default, skip_serializing_if = "MapSlot::is_empty")] pub param_defs: MapSlot<String, ShaderParamDef>` and expose it as a map slot. `MapSlot` should serialize to authored TOML like a normal map. Do not require `examples/basic` to use it in the first slice; add a focused source-slot test with one param def.

### Q4. How should real `FixtureDef.mapping` be represented in M2?

Context: mapping has nested vectors and enums. The slot system currently has records/maps/enums/options/value leaves, not arrays.

Answer: do not introduce arrays. Prefer `MapSlot<u32, PathSpec>` for path collections, with no custom serde to preserve old array syntax. `ValueSlot<MappingConfig>` is acceptable as a fallback if structured mapping makes M2 too large, but the architectural direction is keyed maps.

### Q5. Should `TextureDef` expose flat `width`/`height` slots or one semantic `size` slot?

Context: slot design favors versioning a logically complete value such as `size`, but real TOML has flat fields and changing TOML shape is not required for this milestone.

Answer: `size` is not mandatory, but it would be useful for UI work because it
would show generic rendering of an opaque semantic object. It is reasonable for
M2 to change `TextureDef` from flat `width` / `height` to a semantic `size`
slot if that remains a low-risk source schema cleanup.

### Q6. Where should source shape registration live?

Context: `lpc-source` owns the defs and can define the shapes; `lpc-engine` loads project artifacts and knows artifact content frames.

Suggested answer: after M1.3, `lpc-source` should use the generated
`OUT_DIR` slot-shape bootstrap. The crate owns static source root shapes and
will expose generated helpers such as
`lpc_source::slot_shapes::register_all_static_slot_shapes` and
`lpc_source::slot_shapes::ensure_static_slot_shape`. Engine can call those
later, but M2 source tests can call them directly.

### Q7. Should M2 convert all real source defs or only one vertical slice?

Context: converting only one or two defs is less churn, but it leaves production source partially split between plain structs and slot-aware domain objects.

Answer: prefer all real source defs in M2. It is acceptable to phase the implementation internally, but the milestone should end with `ProjectDef`, `NodeInvocation`, `TextureDef`, `ShaderDef`, `OutputDef`, and `FixtureDef` exposed through the unified slot-domain model.

## Suggested First Deliverable

Create source-side tests that:

1. load `examples/basic/project.toml` and each referenced child TOML,
2. register source shapes,
3. expose each source def as a slot root with a fixed content frame,
4. snapshot roots through `lpc-wire::slot::access_sync` or an equivalent source adapter,
5. assert and print paths such as:
   - `project#nodes[shader].artifact`
   - `shader#glsl_path`
   - `shader#texture_loc`
   - `shader#glsl_opts.add_sub`
   - `output#pin`
   - `output#options.some.brightness`
   - `fixture#mapping`

## Validation Notes

Use targeted host commands; do not run `cargo test --workspace`.

Likely M2 validation:

```bash
cargo fmt
cargo test -p lpc-model -p lpc-source -p lpc-wire --lib --tests
cargo check -p lpc-engine -p lpa-client -p lpa-server -p lp-cli
```

If source slot roots touch project loading:

```bash
cargo test -p lpc-engine --lib --tests
```
