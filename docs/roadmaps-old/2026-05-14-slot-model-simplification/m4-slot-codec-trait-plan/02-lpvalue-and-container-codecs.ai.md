# Phase 2: LpValue And Container Codecs

## Scope Of Phase

Move primitive field behavior into reusable codec impls.

In scope:

- implement `SlotCodec for ValueSlot<T>` where `T: SlotValue`
- implement map and option slot codecs
- add `LpType`-driven `LpValue` read/write helpers
- add focused tests for scalar, vector, matrix, struct, map, and option shapes

Out of scope:

- generated record codecs
- full enum codegen
- changing real domain loading paths

## Code Organization Reminders

- Put generic `LpValue` helpers in `lpc-model/src/slot_codec/slot_value_codec.rs`.
- Keep container impls near the `SlotCodec` trait or in a clearly named submodule.
- Avoid adding special files for each semantic type. Semantic leaf behavior should come from `SlotValue`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add helpers:

```rust
pub fn read_lp_value<S>(
    ty: &LpType,
    value: ValueReader<'_, '_, S>,
) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource;

pub fn write_lp_value<W>(
    value: SlotValueWriter<'_, W>,
    ty: &LpType,
    lp_value: &LpValue,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite;
```

Support at least the shapes exercised by the mockup:

- `String`
- `I32`
- `U32`
- `F32`
- `Bool`
- `Vec2`
- `Vec3`
- `Vec4`
- `Mat3x3`
- `Struct`

Add other `LpType` variants if they are straightforward, but do not get stuck on exotic cases unless tests require them.

Implement:

```rust
impl<T> SlotCodec for ValueSlot<T>
where
    T: SlotValue,
{
    ...
}
```

Read path:

1. read `LpValue` according to `T::value_shape().ty`
2. convert with `T::from_lp_value`
3. wrap with `ValueSlot::new`

Write path:

1. convert with `self.value().to_lp_value()`
2. write according to `T::value_shape().ty`

Implement `SlotCodec` for:

- `MapSlot<String, V>`
- `MapSlot<u32, V>`
- `OptionSlot<T>`

For `OptionSlot<T>`:

- reading a present property returns `OptionSlot::some(T::read_slot(value)?)`
- `should_write_slot` returns `self.data.is_some()`
- writing a `Some` delegates to the inner value
- writing a `None` may return `SlotWriteError::Serialize`; generated record writers should skip it via `should_write_slot`

## Validate

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-model value_slot
cargo check -p lpc-model --no-default-features
```
