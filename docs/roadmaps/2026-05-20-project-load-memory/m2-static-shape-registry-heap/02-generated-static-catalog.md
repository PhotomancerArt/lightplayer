# Phase 02: Generated Static Catalog

## Scope Of Phase

Extend slot macros/codegen so registered static shapes expose generated
borrowed descriptors and catalog lookup functions.

Out of scope:

- Removing engine bootstrap registration.
- Changing normal wire protocol.

## Code Organization Reminders

- Keep generator rendering logic in `lpc-slot-codegen/src/render/slot_shapes.rs`.
- Keep macro descriptor emission close to existing shape emission.
- Tests stay at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report the blocker.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/render/slot_shapes.rs`
- `lp-core/lpc-slot-macros/src/slotted_record.rs`
- `lp-core/lpc-slot-macros/src/slotted_wrapper.rs`
- `lp-core/lpc-slot-macros/src/slotted_enum.rs`
- `lp-core/lpc-slot-macros/src/value.rs`
- `lp-core/lpc-model/src/lib.rs`

Generated API target:

```rust
pub fn static_slot_shape(id: SlotShapeId) -> Option<&'static StaticSlotShapeDescriptor>;
pub fn static_slot_shape_name(id: SlotShapeId) -> Option<&'static str>;
pub fn static_slot_shape_ids() -> &'static [SlotShapeId];
pub fn create_static_slot_default(id: SlotShapeId) -> Option<SlotFactory>;
```

Every registered generated static shape must be addressable through the catalog.
Avoid an embedded fallback that materializes all static shapes into the registry.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-model slot_shape
```
