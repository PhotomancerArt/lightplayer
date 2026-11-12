# Phase 1: JSON Writer

## Scope Of Phase

Implement generic shape-driven JSON writing in `lpc-model`.

In scope:

- add `dynamic_slot_writer.rs`
- add a semantic writer error
- add JSON shape walking over `SlotShape` + `SlotDataAccess`
- add registry APIs for writing a root slot object and arbitrary slot data to
  JSON
- add focused `lpc-model` unit tests for records, maps, enums, options, refs,
  and value leaves

Out of scope:

- TOML writing
- deleting old `SlotCodec`
- updating real engine/wire callers

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep the public writer functions at the top of `dynamic_slot_writer.rs`.
- Put helper functions below the main shape walk.
- Tests belong at the bottom of the file.
- Do not add generated code.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot_codec/slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/slot_value_codec.rs`

Add public functions similar to:

```rust
pub fn write_dynamic_slot_json<W>(
    registry: &SlotShapeRegistry,
    root: &dyn SlotAccess,
    out: W,
) -> Result<W, SlotWriteError<W::Error>>
where
    W: SlotWrite;

pub fn write_slot_data_json_value<W>(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite;
```

The core helper should walk:

- `SlotShape::Ref`: resolve through registry
- `SlotShape::Unit`: write `{}` when explicitly written
- `SlotShape::Value`: call `write_lp_value`
- `SlotShape::Record`: write object fields in shape order
- `SlotShape::Map`: write object properties by key text
- `SlotShape::Enum`: write object with `kind`, then payload
- `SlotShape::Option`: write `Some` payload or root `null`

For record fields:

- omit `None` option fields
- if easy, omit empty records/maps in JSON
- do not make this clever; a simple local `should_omit_json_field` helper is
  enough

Add a writer semantic error:

- missing shape
- missing referenced shape
- shape/data mismatch
- missing record data
- unknown enum variant
- unsupported enum payload shape

Preferred shape:

```rust
pub enum SlotDataWriteError {
    MissingShape(SlotShapeId),
    MissingReferencedShape(SlotShapeId),
    ShapeDataMismatch { message: String },
    UnknownVariant { variant: String },
}
```

Then add a `SlotWriteError::SlotData(SlotDataWriteError)` or equivalent mapping
so JSON callers get readable errors.

Registry APIs:

- `SlotShapeRegistry::write_slot_json`
- `SlotShapeRegistry::write_slot_json_value`

Tests:

- write a dynamic record to JSON
- write a map to JSON
- write enum `kind` plus payload to JSON
- omit record `None` option fields
- root `None` writes `null`
- ref shapes resolve correctly
- shape/data mismatch reports a useful error

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model dynamic_slot_writer
cargo test -p lpc-model slot_value_codec
```
