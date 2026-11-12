# 01 - Factory Types And Dynamic Object

## Scope of phase

Add the core factory types and `DynamicSlotObject`.

In scope:

- New `lpc-model/src/slot/slot_factory.rs`.
- `SlotFactory`, `SlotFactoryFn`, `SlotFactoryError`.
- `DynamicSlotObject`.
- Exports from `slot/mod.rs` and `lib.rs`.

Out of scope:

- Registry integration.
- Static codegen changes.
- Dynamic recursive data construction beyond a placeholder factory target if
  needed for type compilation.

## Code organization reminders

- Prefer one main concept per file.
- Keep public types near the top and helpers below.
- Tests go at the bottom of the file.
- Do not put factory code into `slot_shape_registry.rs` if it can live cleanly
  in `slot_factory.rs`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_factory.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/slot_mut_access.rs`

Define:

```rust
pub type SlotFactoryFn =
    fn(&SlotShapeRegistry, SlotShapeId) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;

#[derive(Clone, Copy)]
pub enum SlotFactory {
    Static(SlotFactoryFn),
    Dynamic,
    Unsupported,
}
```

Define `DynamicSlotObject`:

```rust
pub struct DynamicSlotObject {
    shape_id: SlotShapeId,
    data: SlotData,
}
```

Implement:

- `DynamicSlotObject::new`
- `DynamicSlotObject::into_data`
- `DynamicSlotObject::data_ref`
- `SlotAccess`
- `SlotMutAccess`

Do not derive serde for `DynamicSlotObject`; this is a runtime root wrapper.

Add a small unit test that wraps a `SlotData::Value` or empty record and can be
read/mutated through `dyn SlotAccess` / `dyn SlotMutAccess`.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model slot_factory
```
