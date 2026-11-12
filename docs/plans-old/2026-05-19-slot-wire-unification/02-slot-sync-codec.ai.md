# Phase 2: Slot Sync Snapshot Codec

## Scope Of Phase

Add a purpose-built, lossless slot sync snapshot codec in `lpc-model`.

In scope:

- new `slot_sync_codec` module;
- snapshot JSON writer from `SlotDataAccess`;
- snapshot JSON reader into `SlotData`;
- replacement payload reader/writer usable for non-root patch paths;
- removal of Serde derives and custom Serde helpers from owned `SlotData`
  containers;
- tests covering every `SlotShape` variant and revision preservation.

Out of scope:

- changing project-read writers to use the new codec;
- removing old `WireSlotData` helpers;

## Code Organization Reminders

- Use filesystem-oriented modules:
  - `slot_sync_codec/mod.rs`
  - `slot_sync_codec/snapshot_writer.rs`
  - `slot_sync_codec/snapshot_reader.rs`
- Keep public entry points near the top of each file.
- Keep syntax helpers lower in the files.
- Put tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot_codec/slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/json_syntax_source.rs`
- `lp-core/lpc-model/src/slot_codec/slot_reader.rs`

Design:

- Keep `SlotData` as the in-memory owned snapshot type.
- Remove `serde::Serialize` / `serde::Deserialize` derives from:
  - `SlotData`
  - `SlotRecord`
  - `SlotMapDyn`
  - `SlotMapKey`
  - `SlotMapEntry` if it remains useful at all
  - `SlotEnum`
  - `SlotOptionDyn`
- Remove the `slot_map_entries` Serde helper module if no non-Serde use remains.
- Add codec entry points with names along these lines:

```rust
pub fn write_slot_snapshot_json_value<W>(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite;

pub fn read_slot_snapshot_json_data(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    json: &str,
) -> Result<SlotData, SlotSnapshotReadError>;
```

The exact names can change if local naming suggests a better fit, but keep
"sync" or "snapshot" in the name so callers do not confuse this with authored
SlotCodec.

Format:

- It may start as the current `SlotData` JSON shape because that already carries
  revisions and structural variants.
- Do not rely on Serde-derived `SlotData` serialization in production helpers.
  Implement explicit read/write traversal so this becomes the canonical format,
  not incidental derive output.
- Do not keep Serde derives around "just for tests"; tests should use the new
  codec.
- Use `SlotShape` during read/write validation. A snapshot for a record shape
  must decode as record data; a value shape must decode as value data, and so on.

Required semantics:

- preserve `WithRevision.changed_at` for value leaves;
- preserve `SlotRecord.fields_revision`;
- preserve `SlotMapDyn.keys_revision` and typed `SlotMapKey`s;
- preserve `SlotEnum.variant_revision`, variant name, and payload;
- preserve `SlotOptionDyn.presence_revision`;
- preserve unit revisions;
- resolve `SlotShape::Ref` through the registry;
- report missing referenced shapes clearly.

Tests:

- record with value fields preserves field revisions;
- map with `String`, `I32`, and `U32` key domains preserves keys and entries;
- enum with record payload preserves variant revision and payload data;
- option `None` and `Some` preserve presence revision;
- `SlotShape::Ref` decodes through referenced shape;
- malformed data reports the expected shape/data mismatch.
- compile should fail if any production code still tries to serialize or
  deserialize `SlotData` with Serde.

## Validate

```bash
cargo test -p lpc-model slot_sync_codec
rg -n "serde::Serialize|serde::Deserialize|#\\[serde|SerializeSeq|Deserializer|Serializer" lp-core/lpc-model/src/slot/slot_data.rs
```

The `rg` command should produce no matches.
