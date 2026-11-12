# Phase 02: Dynamic Reader Core

## Scope Of Phase

Add the generic dynamic reader implementation in a new focused file.

In scope:

- Create `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`.
- Export appropriate APIs from `lp-core/lpc-model/src/slot_codec/mod.rs`.
- Implement `read_dynamic_slot`.
- Implement the recursive shape/data walker for records, maps, enums, options,
  refs, and values.
- Add focused `lpc-model` tests using small test-only shapes where useful.

Out of scope:

- Registry convenience wrappers.
- Mockup integration tests.
- Generic dynamic writing.
- Validation of default sentinel values.

## Code Organization Reminders

- Keep the headline API near the top of `dynamic_slot_reader.rs`.
- Place helper structs/functions below the public API.
- Keep tests at the bottom.
- Prefer one small helper per shape category:
  - `read_record`
  - `read_map`
  - `read_enum`
  - `read_option`
  - `read_value`

## Sub-Agent Reminders

- Do not commit.
- Do not route the whole implementation through path strings unless direct
  mutable access proves impossible.
- Do not make missing fields errors.
- Unknown fields must remain errors.
- If `SlotDataMutAccess` lacks a tiny primitive needed for direct traversal,
  add the smallest targeted helper instead of redesigning mutation.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-model/src/slot/slot_mut_access.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`

Core algorithm:

```rust
pub fn read_dynamic_slot<S>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    value: ValueReader<'_, '_, S>,
) -> Result<Box<dyn SlotMutAccess>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let shape = registry.get(&shape_id).ok_or(...)?;
    let mut object = registry.create_default(shape_id).map_err(...)?;
    apply_reader_to_slot(object.data_mut(), shape, registry, value)?;
    Ok(object)
}
```

Important behavior:

- `Ref`: resolve the referenced shape and recurse.
- `Record`: object properties map to `SlotFieldShape.name`; unknown fields
  error; missing fields stay default.
- `Map`: object properties become `SlotMapKey`s according to
  `SlotMapKeyShape`; insert default entry then recurse into the entry.
- `Enum`: require `kind` as first property; valid values come from variants;
  switch to selected variant default; read remaining object fields into the
  active variant payload.
- `Option`: a present property/value means `Some(default)` then read payload;
  absent property remains `None`.
- `Value`: use `read_lp_value(shape.ty, value)` and set via
  `SlotValueMutAccess::set_lp_value`.
- `Unit`: support a minimal representation if straightforward; otherwise keep
  the tests focused on non-unit payloads and leave a note.

Errors from `SlotFactoryError` and `SlotMutationError` should be mapped to
`SyntaxError` with clear messages.

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model dynamic_slot_reader
cargo test -p lpc-model slot_factory
cargo test -p lpc-model slot_mutation
```
