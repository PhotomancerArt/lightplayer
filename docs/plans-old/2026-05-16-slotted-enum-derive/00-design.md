# Slotted Enum Derive Design

## Scope

Teach `#[derive(Slotted)]` to generate slot machinery for enum payloads:

- unit variants
- one-field tuple variants
- named-field variants

Then replace manual enum slot machinery in the mockup and real model for:

- mockup `NodeDef`
- mockup `MappingConfig`
- mockup `PathSpec`
- real `NodeDef`
- real `MappingConfig`
- real `PathSpec`

Out of scope:

- multiple-field tuple variants
- deriving `SlotValue` for atomic enums
- making raw Rust enums full `SlotAccess` roots
- changing the dynamic reader's discriminator field away from `kind`
- removing serde annotations broadly from `lpc-model`

## File Structure

```text
lp-core/lpc-slot-macros/src/
  lib.rs
  attr.rs
  slotted.rs              # dispatcher for struct/enum derive
  slotted_record.rs       # named-field struct support
  slotted_wrapper.rs      # one-field tuple struct support
  slotted_enum.rs         # enum support

lp-core/lpc-model/tests/
  slot_record_derive.rs   # existing record/wrapper tests
  slotted_enum_derive.rs  # new enum derive tests

lp-core/lpc-slot-mockup/src/source/
  node_def.rs
  mapping.rs

lp-core/lpc-model/src/nodes/
  node_def.rs
  fixture/mapping.rs

docs/design/slots/
  overview.md
```

The exact file names can vary slightly during implementation, but avoid growing
`record.rs` into a record/wrapper/enum mega-file.

## Architecture Summary

`Slotted` becomes the single author-facing derive for structured slot objects:

- named-field structs become slot records
- one-field tuple structs become transparent slot wrappers
- enums become slotted enum payloads

The runtime layering remains:

```text
NodeArtifact
  derives Slotted as tuple wrapper
  owns static artifact shape id/factory boundary
  wraps EnumSlot<NodeDef>

EnumSlot<NodeDef>
  owns active variant revision
  exposes enum data and mutation

NodeDef
  derives Slotted as enum payload
  owns variant cases and payload field access
```

For named-field variants, the enum itself implements `SlotRecordAccess` and
`SlotRecordMutAccess`, with field indices interpreted in the active variant's
record payload.

For one-field tuple variants, the enum delegates payload shape/data to the
single field's `FieldSlot` / `FieldSlotMut`.

For unit variants, the enum exposes unit data. `EnumSlot<T>` remains responsible
for stamping and mutating the active variant revision.

## Generated Enum Behavior

For:

```rust
#[derive(Slotted)]
pub enum MappingConfig {
    #[default]
    Unset,
    PathPoints {
        paths: MapSlot<u32, EnumSlot<PathSpec>>,
        sample_diameter: PositiveF32Slot,
    },
}
```

Generate:

- `impl Default for MappingConfig`
- `impl SlotEnumShape for MappingConfig`
- `impl SlottedEnum for MappingConfig`
- `impl SlottedEnumMut for MappingConfig`
- `impl SlotRecordAccess for MappingConfig`
- `impl SlotRecordMutAccess for MappingConfig`

Variant shape rules:

- unit: `slot::shape::unit()`
- single tuple: `<FieldTy as FieldSlot>::slot_field_shape()`
- named fields: `slot::shape::record(vec![field(...), ...])`

Variant data rules:

- unit: `SlotDataAccess::Unit(Revision::default())`
- single tuple: delegate to `FieldSlot`
- named fields: `SlotDataAccess::Record(self)`

Default rules:

- `#[default]` on a variant chooses that variant
- exactly one variant may default implicitly
- otherwise compile error
- default payloads use `Default::default()` for every payload field
- domain enums may and often should add a neutral unit variant like `Unset`
  when no data-bearing variant is semantically valid as an empty default

## Attribute Surface

Container attributes:

- existing `#[slot(shape_id = "...")]` remains for static shape ids

Variant attributes:

- `#[default]` chooses the enum default variant
- `#[slot(name = "...")]` remains an escape hatch for non-standard variant slot/discriminator names

Field attributes:

- named variant fields reuse existing record field attributes:
  - `#[slot(name = "...")]`
  - `#[slot(value = ...)]`
  - `#[slot(leaf = ...)]`
  - `#[slot(record)]`
  - `#[slot(map(...))]`
  - `#[slot(option_ref = "...")]`

## Validation Targets

Minimum validation:

- `cargo test -p lpc-model --features derive --test slotted_enum_derive`
- `cargo test -p lpc-model nodes::node_def`
- `cargo test -p lpc-model nodes::fixture::mapping`
- `cargo test -p lpc-slot-mockup`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-engine project_loader`
