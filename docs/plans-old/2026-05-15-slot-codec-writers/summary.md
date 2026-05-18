# Slot Codec Writers Plan Summary

## What Was Built

- Added shape-driven JSON writing in `lpc-model::slot_codec`.
- Added shape-driven TOML writing in `lpc-model::slot_codec`.
- Added registry write APIs for root slot objects and arbitrary slot data:
  - `write_slot_json`
  - `write_slot_json_value`
  - `write_slot_toml`
  - `write_slot_toml_data`
- Added clearer slot-data writer errors for shape/data mismatches.
- Added JSON `null` support to the low-level slot writer.
- Added model tests for dynamic writer records, maps, enums, options, refs,
  TOML values, products, and mismatch errors.
- Expanded mockup dynamic codec tests with JSON/TOML write and round-trip
  coverage for static objects, enum payloads, and registered dynamic shapes.

## Decisions For Future Reference

### JSON Streaming

- **Decision:** JSON writing streams through `SlotWriter`.
- **Why:** This preserves the embedded memory goal and avoids building a JSON
  tree for wire messages.
- **Rejected alternatives:** Build `serde_json::Value` or another generic tree.
- **Revisit when:** Only if a future JSON formatter needs whole-document layout.

### TOML Tree

- **Decision:** TOML writing returns `toml::Value`.
- **Why:** Authored TOML is small and TOML table layout is awkward to stream
  without backtracking.
- **Rejected alternatives:** A streaming TOML writer.
- **Revisit when:** Only if authored disk data becomes unexpectedly large.

### Emission Policy

- **Decision:** Omit `None` record fields, allow JSON to omit easy empty
  containers, and keep TOML explicit for present containers.
- **Why:** JSON bandwidth matters; TOML readability/predictability matters.
- **Rejected alternatives:** Per-field policy metadata before a concrete need.
- **Revisit when:** Real authored/wire formats need field-specific policy.

### Discriminators

- **Decision:** The writer emits discriminators only for slot enum shapes.
- **Why:** Root record shapes should not get hidden type lists or invented
  fields.
- **Rejected alternatives:** Auto-inserting root `kind` for every static record.
- **Revisit when:** Generic wrapper enum loading/writing gets formalized.

## Old Surfaces Ready For Cleanup

- `lpc-slot-mockup/src/tests/generated_shape_codec.rs` has overlapping static
  read/write coverage now that dynamic registry tests can write and read.
- `lpc-slot-mockup/src/tests/storage_codec.rs` can start moving away from old
  `lpc-wire` `encode_slot_data_access_toml` and `write_slot_data_json`.
- `lpc-wire/src/slot/slot_data_json.rs` can be replaced once
  `lpc-engine/src/engine/project_read_stream.rs` uses
  `write_slot_json_value`.

## Remaining Cleanup Blockers

- Old generated/static `SlotCodec` is still compiled and used by existing
  mockup tests/source wrapper code.
- Old `lpc-wire` TOML and JSON SlotData serializers are still exported and used.
- Root `kind` wrapper enum machinery still needs a generic direction before the
  mockup `NodeDef` custom codec can disappear completely.
