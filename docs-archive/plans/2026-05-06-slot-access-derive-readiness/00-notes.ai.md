# Slot Access Derive Readiness Notes

## Scope Of Work

Build the next slot-model step: make Rust-authored slot records ergonomic enough that real LightPlayer defs/config/state can be converted without hand-writing fragile `StaticSlotAccess` / `SlotRecordAccess` boilerplate.

The plan should cover:

- Promoting the useful shape construction helpers from `lpc-slot-mockup/src/model/shape_builder.rs` into `lpc-model`.
- Introducing a derive/codegen path for Rust-authored slot records.
- Reducing or eliminating manual duplication between `register_shape(...)` field order and `SlotRecordAccess::field(index)`.
- Preserving `no_std + alloc` compatibility for runtime/model crates.
- Keeping the first implementation focused on the mockup harness, with real `lpc-source` / `lpc-engine` conversion prepared but not done unless explicitly brought into scope.

## Current State

### Slot Core

- `lpc-model` owns the core slot traits in `lp-core/lpc-model/src/slot/slot_access.rs`:
  - `SlotAccess`
  - `StaticSlotAccess`
  - `SlotDataAccess`
  - `SlotValueAccess`
  - `SlotRecordAccess`
  - `SlotMapAccess`
  - `SlotEnumAccess`
  - `SlotOptionAccess`
- `StaticSlotAccess` currently requires each type to define:
  - `const SHAPE_ID: SlotShapeId`
  - `fn register_shape(registry: &mut SlotShapeRegistry) -> Result<..., ...>`
- `SlotRecordAccess` currently uses indexed field access:
  - `fn field(&self, index: usize) -> Option<SlotDataAccess<'_>>`
  - Field names and field order live in `SlotShape::Record`.
  - The index-based representation is intentional for compact snapshots, but manual impls are fragile.

### Shape Construction

- `lpc-slot-mockup/src/model/shape_builder.rs` has concise helpers:
  - `id`
  - `record`
  - `field`
  - `map`
  - `option`
  - `reference`
  - `variant`
  - `value`
  - `leaf`
  - `unit`
- These helpers make shape code readable and feel like they belong in `lpc-model`.
- Current `SlotShape` constructors are lower-level; moving/adapting the helper vocabulary into `lpc-model` should make both generated and hand-authored shapes cleaner.

### Semantic Leaves

- Recent commit `ae54c7aa Add semantic slot leaf descriptors` added `lp-core/lpc-model/src/slot/slot_leaf.rs`.
- `SlotShape::Value` now carries `SlotValueShape`, which includes:
  - `SlotLeafId`
  - `ModelType`
  - `SlotMeta`
  - `SlotEditorHint`
- Initial semantic leaves include:
  - node refs
  - artifact/source paths
  - `Dim2u`
  - XY
  - affine 2D transform
  - color order
  - ratio / positive f32 / render order hints
- `PinSlot` and compiler-mode semantic slots were explicitly avoided.

### Mockup Pressure Harness

- `lpc-slot-mockup` now has a miniature stack with `model`, `source`, `engine`, `wire`, `view`, and tests.
- It demonstrates:
  - source defs as slot records
  - runtime nodes as slot records
  - dynamic shader params
  - shape registry full sync and incremental sync
  - client-initiated mutation with optimistic locking
  - semantic leaf descriptors flowing into the client mirror
- Manual access impls are now the biggest remaining pain. Examples:
  - `source/fixture_def.rs` repeats fields in `register_shape` and `field(index)`.
  - `source/shader_def.rs` has nested records and map refs.
  - `engine/fixture_node.rs` has typed runtime state records.
  - `source/output_def.rs` and `engine/output_node.rs` are good small record examples.
- The manual pattern is very easy to desync because field order is duplicated.

### Real LightPlayer Types Waiting Behind This

- Real defs live in `lp-core/lpc-source/src/node/...`:
  - `ProjectDef`
  - `ShaderDef`
  - `TextureDef`
  - `FixtureDef`
  - `OutputDef`
- These are still plain serde/source types, not slot-access types.
- `OutputDef` and fixture mappings include enums; derive support will need to handle enum-shaped data eventually.
- Real node/runtime types in `lpc-engine` still use current runtime APIs and are not converted to the slot model yet.

### Macro / Codegen Context

- The workspace already has one proc-macro crate: `lp-shader/lpfn-impl-macro`.
- That macro is only a marker attribute and does not provide a reusable derive pattern.
- There is no current `lpc-model` derive macro crate.
- `lpc-model` is `no_std` by default with an optional `std` feature. A proc-macro crate can use `std` at compile time, but generated code must stay compatible with `no_std + alloc`.

## User Notes That Should Influence The Plan

- “Churn now. This is the time.”
- Aggressive renaming/deletion is acceptable if it establishes the right domain model.
- The goal is readiness to convert real LightPlayer stuff over.
- `shape_builder.rs` feels right and should likely move into `lpc-model`.
- The current manual `StaticSlotAccess` / `SlotRecordAccess` boilerplate is the problem to solve next.
- Keep the model oriented around real LightPlayer domain needs, not a generic abstract demo.

## Open Questions

### Q1. Where should the derive macro live?

Context:

- `lpc-model` must remain usable in `no_std`.
- Proc macros run on the host at compile time, so they can depend on `syn`/`quote`/`proc-macro2` without affecting embedded runtime code generation.
- Rust projects usually put proc macros in a separate crate.

Suggested answer:

- Add a new crate `lp-core/lpc-slot-macros`.
- Add an optional `derive` feature on `lpc-model` that depends on and re-exports the macros.
- Mockup can enable `lpc-model = { ..., features = ["std", "derive"] }`.
- Generated code should refer to `::lpc_model::...` paths and not require `std`.

Decision:

- Accepted.

### Q2. Should the first derive target records only, or records plus enums?

Context:

- Records are the most repeated and fragile boilerplate.
- Enums matter soon: real `OutputDef`, `FixtureMapping`, and possibly config enums need slot access.
- Doing both at once may make the first macro pass too large.

Suggested answer:

- Phase 1 macro support should handle records only.
- Phase 2 should add enum support once record derive is proven.
- Manual enum impls can remain in the mockup until enum derive lands.

Decision:

- Accepted.

### Q3. Should root registration and nested record shape generation be one derive or separate traits?

Context:

- Some records are registered roots, such as `source.shader_param_def` and `source.scalar_hint`.
- Some records are inline only, such as `CompilerOptions`.
- All records need indexed `SlotRecordAccess`.
- Only roots need `StaticSlotAccess`.

Suggested answer:

- Introduce a non-root trait such as `SlotRecordShape` in `lpc-model`:
  - `fn slot_record_shape() -> SlotShape`
- `#[derive(SlotRecord)]` generates `SlotRecordAccess` and `SlotRecordShape`.
- A root attribute, for example `#[slot(shape_id = "source.shader")]`, additionally generates `SlotAccess` and `StaticSlotAccess`.

Decision:

- Accepted with the clearer attribute name `shape_id`.

### Q4. How much shape inference should the derive do?

Context:

- Semantic aliases like `ColorOrderSlot`, `SourcePathSlot`, and `Dim2uSlot` carry obvious shape information conceptually.
- Today they are type aliases to `SlotValue<T>`, so the macro may not be able to distinguish `SourcePathSlot` from `SlotValue<String>` after type alias expansion.
- Raw `SlotValue<String>` often needs explicit shape semantics.

Suggested answer:

- Start explicit for clarity:
  - `#[slot(leaf = source_path_shape())]`
  - `#[slot(value = ModelType::String)]`
  - `#[slot(record)]`
  - `#[slot(map(key = "string", value_ref = "source.shader_param_def"))]`
  - `#[slot(option_ref = "source.scalar_hint")]`
- Add inference later if aliases become newtypes instead of type aliases, or if the macro has enough syntactic information.

Decision:

- Accepted.

### Q5. Should semantic slot aliases become newtypes before derive?

Context:

- Current semantic slots are type aliases, e.g. `pub type ColorOrderSlot = SlotValue<ColorOrderValue>`.
- Type aliases are ergonomic but weaken macro/type-level inference.
- Newtypes would make `ColorOrderSlot` a real distinct type that can implement shape traits directly.

Suggested answer:

- Do not block the derive on this.
- Keep aliases for now.
- Capture “semantic slot newtypes for inference” as future work unless implementation friction becomes severe.

Decision:

- Accepted.

### Q6. Should this plan convert real `lpc-source` types?

Context:

- The user’s goal is readiness to convert real LightPlayer stuff over.
- The mockup is where the slot model is being pressured.
- Real conversion may uncover serde/source compatibility and runtime loader concerns unrelated to macro mechanics.

Suggested answer:

- This plan should not fully convert real `lpc-source` / `lpc-engine`.
- It should add one small compile-only or test-only example that uses the same patterns a real def would use.
- The final phase should document exactly what remains to convert real defs, and leave the mockup mostly free of manual record boilerplate.

Decision:

- Accepted. Goal is to switch the mockup over and make the system ready for real code. Real `lpc-source` / `lpc-engine` conversion is a separate plan.

### Q7. Should shape builder helpers be public API?

Context:

- The helper vocabulary is useful for generated code and manual shape code.
- Public helper names should not feel mockup-specific.

Suggested answer:

- Move helpers into `lpc-model/src/slot/slot_shape_builder.rs`.
- Re-export concise functions under `lpc_model::slot::shape` to avoid polluting the root too much:
  - `shape::id`
  - `shape::record`
  - `shape::field`
  - `shape::map`
  - `shape::option`
  - `shape::reference`
  - `shape::variant`
  - `shape::value`
  - `shape::leaf`
  - `shape::unit`
- Generated code can use fully qualified `::lpc_model::slot::shape::...`.

Decision:

- Accepted.

## Initial Validation Commands

Likely validation set for this plan:

```bash
cargo test -p lpc-model
cargo check -p lpc-model --no-default-features
cargo check -p lpc-model --features schema-gen
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo check -p lpc-view
cargo check -p lpc-wire --features schema-gen
git diff --check
```
