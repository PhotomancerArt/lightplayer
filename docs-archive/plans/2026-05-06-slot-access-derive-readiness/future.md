# Future Work

## Enum Derive

- **Idea:** Add a derive for slot enums once record derive is proven.
- **Why not now:** Records are the immediate boilerplate and field-order correctness problem.
- **Useful context:** `lpc-slot-mockup/src/source/fixture_def.rs` has `FixtureMapping`, and real `lpc-source` has enum-shaped definitions such as `OutputDef`.

## Semantic Slot Newtypes

- **Idea:** Replace some semantic slot type aliases with real newtypes so derives can infer shapes from field types.
- **Why not now:** Explicit field annotations are clearer for the first derive slice and avoid blocking on type design.
- **Useful context:** Current aliases such as `ColorOrderSlot = SlotValue<ColorOrderValue>` are ergonomic but not strongly distinct to macro parsing.

## Real Source And Runtime Conversion

- **Idea:** Convert real `lpc-source` defs and `lpc-engine` runtime state/output structs to derived slot access.
- **Why not now:** This plan prepares the tooling and validates it in the mockup first.
- **Useful context:** Real defs live under `lp-core/lpc-source/src/node`, and runtime/node types live under `lp-core/lpc-engine/src/nodes`.
