# Slot Wire Unification Design

## Scope

Make detailed project-read node slot roots and patch replacements use one
canonical sync snapshot format from server writer to client mirror. The format
must be lossless for `SlotData` semantics, including revisions, while remaining
streamable for ESP32/server transports.

This plan also removes Serde bindings from `SlotData` and its owned containers,
fixes the shape paging bug that can omit required root shapes, and adds debug UI
regressions for slot roots and resource payload access.

## File Structure

```text
lp-core/
  lpc-model/src/
    lib.rs
    slot/slot_data.rs
    slot_sync_codec/
      mod.rs
      snapshot_reader.rs
      snapshot_writer.rs

  lpc-wire/src/slot/
    mod.rs
    sync.rs
    access_sync.rs

  lpc-view/src/slot/
    mirror.rs
    apply.rs

  lpc-engine/src/engine/
    project_read_nodes.rs
    project_read_stream.rs
    project_read_shapes.rs

lp-cli/src/debug_ui/
  ui.rs

docs/plans/2026-05-19-slot-wire-unification/
  00-notes.md
  00-design.md
  01-shape-paging-regression.md
  02-slot-sync-codec.md
  03-project-read-writers.md
  04-client-view-strict-decode.md
  05-debug-ui-regressions.md
  06-cleanup-validation.md
```

## Architecture Summary

There are two distinct serialization domains:

```text
Authored/value payloads
  typed SlotAccess
      |
      v
  SlotCodec JSON/TOML
      |
      v
  concise shape-owned authoring syntax

Sync snapshots
  typed SlotAccess or owned SlotData
      |
      v
  Slot sync snapshot codec
      |
      v
  lossless client mirror syntax with revisions
```

`SlotCodec` stays the authored/value codec. It should not be the project-read
client sync snapshot codec unless it grows explicit sync metadata later.

The new slot sync snapshot codec writes a full dynamic snapshot through a
registered `SlotShape`, directly from `SlotDataAccess`, without requiring an
intermediate `SlotData` allocation in the streaming path. Its reader produces
`SlotData` for `lpc-view`.

`WireSlotRootSnapshot.data` remains raw JSON internally for transport
efficiency, but its public helpers and docs define it as one format only:
slot-sync snapshot JSON. The client mirror calls one reader. There is no
SlotCodec fallback and no Serde fallback.

`WireSlotPatch` replacements also use the same snapshot payload format. This is
what allows `SlotData` itself to drop `Serialize`/`Deserialize` derives while
remaining the in-memory mirror and resolver payload type.

## Main Components

### Slot Sync Snapshot Codec

New module: `lp-core/lpc-model/src/slot_sync_codec/`.

Responsibilities:

- write a lossless snapshot from `(registry, shape_id, SlotDataAccess)` to a
  `SlotWrite` sink;
- read a snapshot from `(registry, shape_id, JSON source)` into `SlotData`;
- optionally write/read a replacement for a non-root shape, so patches use the
  same codec;
- preserve value revisions and container revisions;
- validate shape/data mismatches against `SlotShape`;
- handle refs, records, maps, enums, options, units, and values;
- use deterministic field/key ordering compatible with the existing model.

The JSON shape can start close to current `SlotData` JSON because that already
captures the needed semantics. The important change is ownership: the codec is a
purpose-built sync codec, not Serde-derived incidental JSON and not authored
SlotCodec JSON. After this codec owns the format, remove Serde derives and
custom Serde helper modules from `SlotData` and its owned containers.

### Wire Slot Payload Helpers

Files:

- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`

Responsibilities:

- rename or document `WireSlotData` as sync snapshot payload data;
- provide construction through the sync codec only;
- provide reading through the sync codec only;
- update `build_slot_full_sync`, `build_slot_roots_snapshot`, and
  `snapshot_slot_root` call sites as needed;
- replace `WireSlotChange::Replace(SlotData)` with a raw snapshot payload shape
  that can be decoded using the root shape plus patch path.

### Project Read Writers

Files:

- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

Responsibilities:

- make allocated `Engine::read_project` and streaming
  `Engine::write_project_read_json` emit the same slot root data syntax;
- avoid per-root `SlotData` allocation in the streaming path when possible;
- add tests that compare or jointly validate both paths;
- assert that SlotCodec JSON is no longer accepted by the client sync reader.

### Client Mirror

Files:

- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-view/src/slot/apply.rs`

Responsibilities:

- remove fallback decoding;
- improve error messages so they say "slot sync snapshot" rather than
  SlotCodec-or-SlotData;
- keep `SlotData` as the in-memory mirror format;
- preserve revision values used by `prepare_set_value`.

### Shape Paging

Files:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-cli/src/debug_ui/ui.rs`

Responsibilities:

- define `ShapeReadResult.next` as the value the client sends back as `after`;
- make `snapshot_page` return the last included id when more entries remain;
- test with `limit = 1` that paging reconstructs every registry entry without
  skips;
- keep debug UI shape-page accumulation behavior.

### Debug UI Regression Coverage

Files:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

Responsibilities:

- prove the debug UI can page all shapes, then request and apply node slot roots;
- prove selecting a resource requests and caches its payload after node slots
  are present;
- keep resource payload streaming on the existing manual base64 writer.

## Expected Outcomes

- A `WireSlotRootSnapshot` has one data format.
- `SlotMirrorView` has one root decode path.
- Project-read allocated and streaming producers no longer disagree.
- Full sync preserves the revisions needed for mutation conflict checks.
- Patch replacements no longer require `SlotData` Serde.
- `SlotData` and owned slot data containers no longer derive Serde.
- Missing shape errors indicate a real registry sync problem, not a skipped
  cursor or format ambiguity.
- Resource payload access recovers because slot apply no longer aborts every
  response.

## Non-Goals

- Do not disable or gate the compiler.
- Do not remove `SlotData`.
- Do not remove Serde from non-slotted protocol envelopes as part of this plan.
- Do not make authored TOML less SlotCodec-owned.
- Do not rewrite resource payload streaming.
