# Phase 1: Center `SlotValue`

## Scope Of Phase

Make `SlotValue` the obvious public contract for semantic leaf values.

In scope:

- Tighten docs around `SlotValue`, `ToLpValue`, `FromLpValue`, and `ValueSlot<T>`.
- Ensure `ValueSlot<T>` impls consistently use `T: SlotValue` where semantic shape is needed.
- Keep `ToLpValue` and `FromLpValue` as lower-level conversion traits.
- Add tests that prove `ValueSlot<T: SlotValue>` exposes shape, revision, and `LpValue` data generically.

Out of scope:

- Adding the derive macro.
- Converting all semantic slot files.
- Custom disk/wire codec implementation.

## Code Organization Reminders

- Keep `SlotValue` concepts in `lp-core/lpc-model/src/slot/slot_value.rs`.
- Keep storage/container behavior in `lp-core/lpc-model/src/slot/value_slot.rs`.
- Do not move unrelated slot access code.
- Put tests at the bottom of the files they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-core/lpc-model/src/slot/slot_value.rs`
- `lp-core/lpc-model/src/slot/value_slot.rs`

Expected shape:

```rust
pub trait SlotValue: Sized + ToLpValue + FromLpValue {
    const SHAPE_ID: SlotShapeId;

    fn value_shape() -> SlotValueShape;
}
```

Do not fold `ToLpValue` / `FromLpValue` into `SlotValue` yet. The user wants `SlotValue` as the main concept, but agrees the conversion traits may still have use.

Clarify docs:

- `SlotValue` is the semantic leaf payload.
- `ValueSlot<T>` is the revision-tracked slot leaf container.
- A slot leaf is one complete `LpValue` payload; sub-fields inside that `LpValue` are not independently addressable slots.

Review `ValueSlot<T>`:

- `SlotValueAccess` may only require `T: ToLpValue`.
- `FieldSlot` should require `T: SlotValue`.
- serde passthrough may remain while authored TOML still depends on serde in places.

Add/adjust tests proving:

- `ValueSlot<RatioLike>` can expose revision and `LpValue`.
- `ValueSlot<RatioLike>` gets field shape from `T::value_shape`.
- raw primitive `ValueSlot<f32>` still works.

## Validate

```bash
cargo fmt
cargo test -p lpc-model slot_value
cargo test -p lpc-model value_slot
```
