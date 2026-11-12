# Slot Codec Writers Design

## Scope

Add the generic writer half of the slot codec system in `lpc-model`.

The writer should serialize any slot object supported by `SlotAccess`, its
registered `SlotShape`, and the `SlotShapeRegistry`. It should produce both:

- streaming JSON for wire use
- `toml::Value` for authored disk use

This is prep work for later removal of old generated/static `SlotCodec`
machinery and old `lpc-wire` `SlotData` serializers.

## File Structure

```text
lp-core/lpc-model/src/slot_codec/
  dynamic_slot_reader.rs
  dynamic_slot_writer.rs       new shared shape-driven writer walker
  json_syntax_source.rs
  mod.rs                       exports writer APIs
  slot_reader.rs
  slot_value_codec.rs          leaf LpValue write/read helpers
  slot_writer.rs               streaming JSON output facade
  syntax.rs
  toml_syntax_source.rs
  toml_value_writer.rs         optional split if TOML helpers get large

lp-core/lpc-model/src/slot/
  slot_shape_registry.rs       registry-level write entry points

lp-core/lpc-slot-mockup/src/tests/
  dynamic_slot_codec.rs        JSON/TOML writer and round-trip coverage
```

## Architecture Summary

The writer mirrors the dynamic reader:

```text
SlotShapeRegistry
  ├─ read_slot_json/read_slot_toml/read_slot_from
  └─ write_slot_json/write_slot_toml/write_slot_json_value/write_slot_toml_data

dynamic_slot_writer
  ├─ resolves SlotShape::Ref through the registry
  ├─ walks records, maps, enums, options, units, and value leaves
  ├─ writes JSON through SlotValueWriter
  └─ builds TOML through toml::Value

slot_value_codec
  ├─ write_lp_value for typed leaves
  └─ write_untyped_lp_value for literal LpValue payloads
```

JSON remains streaming. It should not build an intermediate syntax tree. This is
important for embedded memory pressure.

TOML returns `toml::Value`. Authored TOML data is expected to be small and TOML
layout wants table backfilling, so a tree is acceptable there.

## Public API Shape

Add registry-centered APIs:

```rust
impl SlotShapeRegistry {
    pub fn write_slot_json<W>(
        &self,
        root: &dyn SlotAccess,
        out: W,
    ) -> Result<W, SlotWriteError<W::Error>>
    where
        W: SlotWrite;

    pub fn write_slot_json_value<W>(
        &self,
        id: SlotShapeId,
        data: SlotDataAccess<'_>,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite;

    pub fn write_slot_toml(
        &self,
        root: &dyn SlotAccess,
    ) -> Result<toml::Value, SlotDataWriteError>;

    pub fn write_slot_toml_data(
        &self,
        id: SlotShapeId,
        data: SlotDataAccess<'_>,
    ) -> Result<toml::Value, SlotDataWriteError>;
}
```

Names can be refined during implementation, but the important part is that
callers write through the registry and slot shape id, not through per-type
serialization impls.

## Format Policy

### Records

- write fields by slot field name
- fields are visited in slot shape order
- `None` option fields are omitted
- JSON may omit empty record/map fields when that is simple and local
- TOML writes present records/maps explicitly

### Maps

- write as JSON object / TOML table
- keys use authored text form:
  - string keys as-is
  - integer keys through decimal text

### Enums

- write an object/table with `kind`
- `kind` is emitted first where the format preserves insertion order
- unit variants write only `kind`
- record variants write `kind` plus payload fields
- unsupported payload shapes should produce a clear semantic writer error

### Options

- record field `None`: omitted
- root JSON `None`: `null`
- root TOML `None`: empty table
- `Some` writes the contained shape

### Values

- JSON uses existing `write_lp_value`
- TOML uses equivalent typed conversion in `lpc-model`
- resource/product leaves must stay supported

### Root Discriminators

The generic writer does not invent a root discriminator for record shapes.
Discriminators are emitted only when the shape is an enum. Wrapper enum support
belongs to the slot enum model, not an external list of type names.

## Errors

The writer needs useful semantic errors for shape/data mismatches, missing
referenced shapes, unknown enum variants, missing record fields, and invalid
TOML value conversion.

Implementation may either:

- add a dedicated `SlotDataWriteError` and map it into `SlotWriteError` for JSON
- or extend `SlotWriteError` with a semantic variant

Preferred direction: add `SlotDataWriteError` for shape/data problems and add a
`SlotWriteError::SlotData(SlotDataWriteError)` variant for JSON.

## Interaction With Cleanup

After this plan lands:

- `lpc-slot-mockup/src/tests/storage_codec.rs` can move from old
  `lpc-wire` writers to registry writers
- `lpc-wire/src/slot/slot_data_json.rs` can be replaced or removed after
  `lpc-engine/src/engine/project_read_stream.rs` moves to
  `write_slot_json_value`
- `lpc-wire/src/slot/authored_toml.rs` can be removed after TOML writer coverage
  is moved to `lpc-model`
- generated/static `SlotCodec` can be deleted after mockup tests rely on
  registry read/write paths
