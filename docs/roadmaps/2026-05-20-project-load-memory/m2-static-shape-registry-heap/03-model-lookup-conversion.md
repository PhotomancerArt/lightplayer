# Phase 03: Model Lookup Conversion

## Scope Of Phase

Convert model-layer traversal from concrete `&SlotShapeRegistry`/`&SlotShape`
assumptions to `SlotShapeLookup` and `SlotShapeView`.

Out of scope:

- Engine bootstrap removal.
- Wire protocol changes.
- Generated slot view direct precompiled accessors.

## Code Organization Reminders

- Update one subsystem at a time: accessor, lookup, factory, codec, mutation.
- Prefer small helper methods on `SlotShapeView` over ad hoc matching.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_accessor.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-model/src/slot/slot_mutation.rs`
- `lp-core/lpc-model/src/slot_codec/*`
- `lp-core/lpc-model/src/nodes/node_def.rs`

Expected changes:

- `SlotAccessor::compile` and `compile_value` accept `&impl SlotShapeLookup`.
- Dynamic slot read/write/default creation traverse `SlotShapeView`.
- `SlotShapeRegistry` remains a valid lookup implementation.
- Tests that build registries keep passing.

## Validate

```bash
cargo test -p lpc-model
```
