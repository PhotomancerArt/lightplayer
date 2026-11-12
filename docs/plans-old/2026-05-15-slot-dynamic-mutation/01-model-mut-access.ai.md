# Phase 1: Model Mutable Slot Access

## Scope Of Phase

Add the mutable slot access traits and primitive/container implementations in `lpc-model`.

In scope:

- Add `slot_mut_access.rs`.
- Export mutable access traits from `slot/mod.rs` and `lib.rs`.
- Implement value, map, option, dynamic data, and simple enum mutable access where the existing owned types make it straightforward.
- Add focused unit tests in `lpc-model`.

Out of scope:

- Runtime client mutation rewiring.
- Codec/deserializer rewiring.
- Enum variant switching.
- Map insert/remove operations.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep mutable access definitions beside, but separate from, read-only `slot_access.rs`.
- Put tests at the bottom of the file.
- Do not create another large catch-all module.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/lib.rs`

Add:

- `SlotMutAccess`
- `SlotDataMutAccess`
- `SlotValueMutAccess`
- `SlotRecordMutAccess`
- `MapSlotMutAccess`
- `SlotEnumMutAccess`
- `SlotEnumDefaultVariant`
- `SlotOptionMutAccess`
- `SlotMutationError`

Expected trait shape:

```rust
pub trait SlotMutAccess: SlotAccess {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}

pub trait SlotValueMutAccess {
    fn changed_at(&self) -> Revision;
    fn set_lp_value(&mut self, revision: Revision, value: LpValue)
        -> Result<(), SlotMutationError>;
}
```

Implement for:

- `ValueSlot<T>` where `T: SlotValue`
- `MapSlot<K, V>` where `K: MapSlotKeyLike`, `V` exposes mutable slot data
- `OptionSlot<T>` where `T` exposes mutable slot data
- owned dynamic `SlotData`, `SlotRecord`, `SlotMapDyn`, `SlotEnum`, `SlotOptionDyn` if useful for tests and future dynamic fallback

Add a mutable map value trait analogous to `SlotMapValueAccess`, probably:

```rust
pub trait SlotMapValueMutAccess {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_>;
}
```

Tests:

- `ValueSlot<f32>` can be set from `LpValue::F32` and rejects `LpValue::Vec3`.
- `MapSlot<String, ValueSlot<f32>>` can mutably access an existing key.
- `OptionSlot<ValueSlot<bool>>` mutably accesses `some` and returns `None` for `none`.
- A manual enum can switch to a default variant and then expose that active payload.

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model slot_mut
```
