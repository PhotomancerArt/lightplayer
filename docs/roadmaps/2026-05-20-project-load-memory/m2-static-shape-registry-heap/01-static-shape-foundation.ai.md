# Phase 01: Static Shape Foundation

## Scope Of Phase

Add borrowed static descriptor types, a static-or-dynamic shape view, and a
lookup trait. Keep existing registry behavior compiling during this phase.

Out of scope:

- Removing static shape registration from the engine.
- Changing wire protocol.
- Fully converting codecs/mutation/accessors.

## Code Organization Reminders

- Prefer granular files: `static_slot_shape.rs`, `slot_shape_view.rs`, and
  `slot_shape_lookup.rs`.
- Keep public types near the top of each file and helpers below.
- Tests stay at the bottom of each file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report the smallest concrete blocker.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`
- `lp-core/lpc-model/src/slot/slot_accessor.rs`

Add:

- borrowed `StaticSlotShapeDescriptor` descriptor family
- `SlotShapeView<'a>`
- `SlotShapeLookup` trait
- a basic `SlotShapeLookup for SlotShapeRegistry` implementation that still
  resolves from the existing owned registry map

Add helpers needed by later phases:

- view accessors for record fields, enum variants, option payloads, refs, and
  value leaves
- static descriptor conversion to owned `SlotShape` for tests/dev fallback only

## Validate

```bash
cargo test -p lpc-model slot_shape
cargo test -p lpc-model slot_accessor
```
