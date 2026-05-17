# M4 Remove Serde From `lpc-model` Design

## Scope

M4 removes Serde, Serde JSON, Schemars, and serde-shaped model behavior from
`lpc-model`.

The major design point is slot metadata. Domain data now has SlotCodec paths,
but slot metadata such as `SlotShape`, `SlotData`, and registry snapshots still
derive Serde. M4 replaces that with explicit metadata codecs that reuse the
existing SlotCodec syntax reader/writer interfaces without modeling metadata as
slotted domain data.

In scope:

- Remove `schema-gen` and all `schemars` usage from `lpc-model`.
- Remove `serde` derives, attributes, imports, manual impls, and tests from
  `lpc-model`.
- Remove `serde_json` test usage from `lpc-model`.
- Remove the `toml/serde` feature from `lpc-model`.
- Add explicit metadata codecs for slot metadata/snapshot types that still
  cross boundaries.
- Replace important serde tests with SlotCodec, metadata codec, or direct parser
  tests.
- Update docs after the cleanup.

Out of scope:

- Removing Serde from other crates unless `lpc-model` API changes require small
  compile fixes.
- Schema versioning.
- Modeling `SlotShape` or `SlotData` as `Slotted`.
- Broad authored syntax redesign.

## File Structure

```text
lp-core/lpc-model/
  Cargo.toml
  src/
    slot_codec/
      mod.rs
      syntax.rs
      slot_reader.rs
      slot_writer.rs
      metadata_codec.rs        # new explicit metadata codecs
    slot/
      slot_shape.rs            # remove serde derives
      slot_shape_registry.rs   # snapshot structs + explicit codec API
      slot_data.rs             # remove serde derives; codec if still needed
      slot_meta.rs
      slot_value.rs
      value_slot.rs
      enum_slot.rs
      slot_name.rs
      slot_path.rs
      slot_ref.rs
      value_ref.rs
    nodes/**                   # remove serde derives/attrs
    slots/**                   # remove serde derives/attrs
    binding/**                 # remove serde impls, keep parsers
    value/**                   # remove serde derives/tests

docs/roadmaps/2026-05-16-slot-codec-serde-removal/m4-remove-serde/
  00-notes.md
  00-design.md
  01-remove-schema-gen.md
  02-add-metadata-codecs.md
  03-remove-domain-serde.md
  04-replace-serde-tests.md
  05-cleanup-validation.md
```

## Architecture Summary

After M4, `lpc-model` has two serialization projections:

1. **Domain SlotCodec**

   Domain records and values serialize through registered slot shapes,
   `SlotAccess`, `SlotMutAccess`, `SlotValue`, and `SlotShapeRegistry`.

   Example:

   ```rust
   NodeDef::read_toml(&registry, text)?;
   NodeDef::write_toml(&registry)?;
   registry.write_slot_json(&project, writer)?;
   ```

2. **Slot Metadata Codec**

   Slot metadata serializes through explicit codecs that use the same syntax
   and writer interfaces, but do not require a registry and do not pretend
   metadata is app-domain slot data.

   Example shape:

   ```rust
   read_slot_shape(value_reader) -> SlotShape
   write_slot_shape(value_writer, &shape)
   read_slot_shape_registry_snapshot(value_reader) -> SlotShapeRegistrySnapshot
   write_slot_shape_registry_snapshot(value_writer, &snapshot)
   ```

The metadata codec is a closed-world codec for a small language:

- `SlotShapeRegistrySnapshot`
- `SlotShapeEntry`
- `SlotShape`
- `SlotVariantShape`
- `SlotFieldShape`
- `SlotValueShape`
- `SlotMeta`
- `ValueEditorHint`
- `SlotData` and related dynamic data types only if an active boundary still
  needs them

## Main Components And Interactions

### `metadata_codec.rs`

Owns explicit read/write functions for slot metadata. It should be structured by
concept, not by format:

- registry snapshot
- shape entries
- shapes
- value shapes/editor hints
- dynamic slot data if needed

It should reuse:

- `SyntaxEventSource`
- `SlotReader`
- `ValueReader`
- `ObjectReader`
- `ArrayReader`
- `SlotWrite`
- `SlotValueWriter`
- `SlotObjectWriter`
- `SlotArrayWriter`

It should not use:

- `SlotShapeRegistry` to interpret metadata
- `Slotted` derives for metadata
- Serde compatibility shims

### Domain Types

Domain types should drop serde derives and attrs. Defaults, optional fields, map
omission, and enum discriminators are owned by SlotCodec and the slot shapes.

Any test that still cares about authored syntax should use:

- `NodeDef::read_toml`
- `NodeDef::write_toml`
- `SlotShapeRegistry::read_slot_toml`
- `SlotShapeRegistry::write_slot_toml`
- `SlotShapeRegistry::read_slot_json`
- `SlotShapeRegistry::write_slot_json`

### Semantic Leaves

Semantic leaves keep parser/display logic and `SlotValue` conversions. They do
not need Serde impls in `lpc-model`.

### Schema Generation

`schema-gen` is removed from `lpc-model`. Future schema work should generate
schemas from slot shapes, likely in host/tooling code outside the embedded model
crate.

## Validation Strategy

Validation should move from narrow serde round trips to crate-level proof:

- `cargo check -p lpc-model`
- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-mockup`
- `cargo test -p lpc-shared project::builder`
- `cargo test -p lpc-engine project_loader`
- `cargo check -p lpc-wire`
- `cargo check -p lpc-source`
- `cargo check -p lpc-view`
- `cargo check -p lpc-shared`
- `git diff --check`

Final search gates:

```bash
rg -n "serde|serde_json|Serialize|Deserialize|schemars|schema-gen" lp-core/lpc-model
rg -n "lpc-model/schema-gen" .
```

Expected final state:

- `lpc-model/Cargo.toml` has no direct `serde`, `serde_json`, or `schemars`.
- `lpc-model` does not enable `toml/serde`.
- No `schema-gen` feature remains on `lpc-model`.
