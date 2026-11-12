# Phase 1: Slot Field Trait

## Scope Of Phase

Add the model-side trait support needed for field inference.

In scope:

- Add `FieldSlot`.
- Implement `FieldSlot` for `ValueSlot<T>`, `MapSlot<K,V>`, and `OptionSlot<T>`.
- Extend map-key support so map key shape can be inferred from `K`.
- Add `SlotMapValueAccess` impls needed for maps/options containing leaf fields.
- Export new traits from `lpc-model`.

Out of scope:

- Macro parsing changes.
- Source def conversion.
- Authored serde for slot wrappers.

## Code Organization Reminders

- Keep trait definitions near related access traits.
- Keep impls near the concrete types they apply to.
- Preserve `no_std + alloc`.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected changes:

- Add:

```rust
pub trait FieldSlot {
    fn slot_field_shape() -> SlotShape;
    fn slot_field_data(&self) -> SlotDataAccess<'_>;
}
```

- Add `MapSlotKeyLike::key_shape() -> SlotMapKeyShape`.
- Implement `FieldSlot` for:
  - `ValueSlot<T>` where `T: SlotLeaf`
  - `MapSlot<K,V>` where `K: MapSlotKeyLike` and `V: FieldSlot + SlotMapValueAccess`
  - `OptionSlot<T>` where `T: FieldSlot + SlotMapValueAccess`
- Implement semantic slot newtypes under `slot/slots/` for domain leaves such as `RatioSlot`, `Dim2uSlot`, `Affine2dSlot`, and `RelativeNodeRefSlot`.
- Implement `SlotMapValueAccess` for value-access fields so semantic leaf slots work inside maps/options.

## Validate

```bash
cargo fmt
cargo test -p lpc-model --lib --tests
```
