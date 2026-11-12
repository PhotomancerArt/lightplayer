# Slot Codec Cleanup Design

## Scope

Remove vestigial slot serialization code now that the generic registry reader
and writer exist.

Keep:

- syntax event sources
- `SlotReader`
- `SlotWriter`
- dynamic reader/writer
- typed `LpValue` helpers
- slot shape/view generation

Remove:

- old static `SlotCodec` trait and generated record codec path
- mockup-only manual codec experiments
- old wire-owned SlotData TOML/JSON walkers after callers move to registry APIs
- compatibility `SlotJson*` names when callers can use neutral names

## File Structure

```text
lp-core/lpc-model/src/slot_codec/
  dynamic_slot_reader.rs
  dynamic_slot_writer.rs
  json_syntax_source.rs
  mod.rs
  slot_reader.rs
  slot_value_codec.rs
  slot_writer.rs
  syntax.rs
  toml_syntax_source.rs

lp-core/lpc-slot-codegen/src/
  render/slot_shapes.rs
  render/slot_views.rs

lp-core/lpc-slot-mockup/src/tests/
  dynamic_slot_codec.rs
  ...non-codec domain tests...

lp-core/lpc-wire/src/slot/
  access_sync.rs
  mutation.rs
  slot_shape_registry_json.rs
  sync.rs
```

## Architecture Summary

Slot serialization flows through the registry:

```text
syntax source -> SlotReader -> registry.apply_reader_to_default_object
SlotAccess + shape registry -> dynamic_slot_writer -> JSON/TOML
```

There should be no generated per-record serialization machinery in the mockup.
The mockup is only a pressure test for the generic system.
