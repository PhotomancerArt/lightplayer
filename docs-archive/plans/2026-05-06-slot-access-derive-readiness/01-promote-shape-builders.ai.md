# Phase 1: Promote Shape Builders

## Scope Of Phase

Move the useful shape-construction helper vocabulary from `lpc-slot-mockup` into `lpc-model`, exposed under `lpc_model::slot::shape`.

In scope:

- Add `lp-core/lpc-model/src/slot/slot_shape_builder.rs`.
- Re-export it as `lpc_model::slot::shape`.
- Update `lpc-slot-mockup` to use `lpc_model::slot::shape` instead of its local `model::shape_builder`.
- Remove the local mockup `shape_builder.rs` once all call sites are migrated.
- Add focused tests/rustdocs for shape helper behavior.

Out of scope:

- Derive macros.
- `SlotRecordShape`.
- Converting any real `lpc-source` / `lpc-engine` code.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related helper functions grouped together.
- Keep tests at the bottom of the file.
- Do not add temporary debug helpers.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-slot-mockup/src/model/shape_builder.rs`
- `lp-core/lpc-slot-mockup/src/model/mod.rs`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
- `lp-core/lpc-slot-mockup/src/engine/*.rs`

Expected `lpc_model::slot::shape` helpers:

- `id(value: &str) -> SlotShapeId`
- `record(fields: Vec<SlotFieldShape>) -> SlotShape`
- `map(key: SlotMapKeyShape, value: SlotShape) -> SlotShape`
- `option(some: SlotShape) -> SlotShape`
- `reference(id: SlotShapeId) -> SlotShape`
- `field(name: &str, shape: SlotShape) -> SlotFieldShape`
- `variant(name: &str, shape: SlotShape) -> SlotVariantShape`
- `value(ty: ModelType) -> SlotShape`
- `leaf(shape: SlotValueShape) -> SlotShape`
- `unit() -> SlotShape`

The helper API may unwrap parse errors, matching the mockup helper style. These are programmer-authored static shapes, so panic-on-invalid-name is acceptable here.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
git diff --check
```
