# Phase 2: Add Slot Metadata Codecs

> Superseded by the measured M4 policy. Keep metadata Serde derives for now;
> add explicit metadata codecs only if firmware bloat or protocol needs point
> at this exact surface.

## Scope Of Phase

Add explicit codecs for slot metadata that currently relies on Serde. These
codecs use the existing SlotCodec syntax reader/writer interfaces but do not
model slot metadata as slotted domain data.

In scope:

- Add `slot_codec::metadata_codec` or equivalent.
- Implement explicit read/write helpers for active metadata boundaries.
- Cover registry snapshots and shape metadata first.
- Cover `SlotData` only if active call sites still require dynamic data
  serialization after M2/M3.
- Add focused tests for metadata codec round trips.

Out of scope:

- Removing all serde derives.
- Redesigning metadata syntax.
- Making `SlotShape`, `SlotData`, or registry snapshots derive `Slotted`.

## Code Organization Reminders

- Prefer one concept section per metadata type family.
- Keep helpers lower in the file when that improves readability.
- Use existing `SlotReader`/`SlotWrite` helpers for object/array/string/number
  plumbing.
- Do not build an in-memory syntax tree except where TOML already requires
  `toml::Value`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-model/src/slot_codec/metadata_codec.rs` new
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/slot_meta.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot_codec/slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/slot_writer.rs`

Preferred public helpers:

```rust
read_slot_shape_registry_snapshot_json(...)
write_slot_shape_registry_snapshot_json(...)
read_slot_shape(...)
write_slot_shape(...)
read_slot_data(...)      // only if still needed
write_slot_data(...)     // only if still needed
```

Exact function names can follow local module conventions, but keep them
discoverable under `slot_codec`.

Expected metadata families:

- `SlotShapeRegistrySnapshot`
- `SlotShapeEntry`
- `SlotShape`
- `SlotFieldShape`
- `SlotVariantShape`
- `SlotValueShape`
- `SlotMeta`
- `ValueEditorHint`
- `SlotData` family if still crossing a boundary

Important constraint:

- These codecs should use `SyntaxEventSource`, `SlotReader`, `ObjectReader`,
  `ArrayReader`, `ValueReader`, and `SlotWrite`.
- They should not use `SlotShapeRegistry` to interpret metadata.

Testing:

- Round-trip a registry snapshot with at least one record shape, one enum shape,
  one map shape, one option shape, and one semantic leaf value shape.
- Round-trip editor hints that are currently used.
- Round-trip `SlotData` only if kept.
- Assert unknown fields and invalid kind values fail clearly.

## Validate

```bash
cargo fmt -p lpc-model
cargo test -p lpc-model slot_codec::metadata_codec
cargo test -p lpc-model slot::slot_shape_registry
cargo check -p lpc-model
```
