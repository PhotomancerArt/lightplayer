# Phase 3: Project Read Writers

## Scope Of Phase

Make all project-read node slot producers emit the same canonical slot sync
snapshot payload.

In scope:

- update allocated `Engine::read_project` node slot snapshots;
- update streaming `Engine::write_project_read_json` node slot snapshots;
- update `lpc-wire` slot helper functions;
- update incremental patch payloads so `WireSlotChange` no longer contains
  `SlotData`;
- add tests that both producer paths agree on root payload semantics.

Out of scope:

- transport raw-frame redesign;
- debug UI behavior changes beyond test support;
- resource payload streaming changes.

## Code Organization Reminders

- Keep project-read writer helpers close to the existing project-read modules.
- If a helper becomes shared between allocated and streaming paths, place it in
  `lpc-wire/src/slot` or a small engine-local helper rather than duplicating
  logic.
- Keep tests at the bottom of their files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`
- `lp-core/lpc-wire/src/slot/mod.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-shared/src/transport/server.rs`

Current split:

- `project_read_nodes.rs` writes root data with
  `wire_slot_data_from_slot_data(&snapshot_slot_root(...))`, producing legacy
  Serde-shaped `SlotData` JSON.
- `project_read_stream.rs` writes root data with
  `SlotShapeRegistry::write_slot_json_value`, producing authored SlotCodec JSON.

Expected result:

- both paths write slot sync snapshot JSON;
- `WireSlotData` helpers expose one construction path;
- patch replacements also write slot sync snapshot JSON;
- no `lpc-wire` type requires `SlotData: Serialize + Deserialize`;
- tests no longer need SlotCodec fallback to apply project-read roots.

Suggested changes:

- Add `wire_slot_snapshot_from_access(registry, shape_id, data)` or similarly
  named helper for allocated paths. It can allocate a raw JSON value, but it
  must use the new sync snapshot writer, not `serde_json::to_raw_value`.
- Add a streaming helper for `JsonValue` in `project_read_stream.rs`, analogous
  to the current `write_slot_codec_json_value`, but using the sync snapshot
  writer.
- Keep `snapshot_slot_root` temporarily if other code needs an owned `SlotData`;
  do not use it as the wire serialization mechanism unless the helper name makes
  that explicit.
- Replace `WireSlotChange::Replace(SlotData)` with something equivalent to
  `Replace(WireSlotData)`, or a more explicit `Replace(WireSlotSnapshotData)`.
  The replacement payload is decoded by the client using the target path's
  resolved `SlotShape`.
- Update `collect_slot_diff` so it writes replacement payloads through the new
  sync snapshot writer instead of constructing `SlotData` for wire serde.
- Update tests in `project_read_stream.rs`:
  - old "slot payloads read through SlotCodec" should become "slot payloads read
    through slot sync snapshot codec";
  - add a test that allocated and streaming project reads produce roots that
    decode to equal `SlotData` for the same project;
  - ensure no test depends on the fallback path.

Transport note:

- The default `ServerTransport::send_project_read` can keep writing JSON then
  deserializing the envelope into `ProjectReadResponse` because `WireSlotData`
  will now contain canonical snapshot JSON in both paths.
- The ESP32 streaming override remains valid because it writes the same
  streaming project-read response directly into the server message frame.

## Validate

```bash
cargo test -p lpc-wire source_slot_sync
cargo test -p lpc-engine project_read_stream
cargo test -p lpc-shared
rg -n "WireSlotChange::Replace\\(SlotData|Replace\\(SlotData\\)|wire_slot_data_from_slot_data|wire_slot_data_to_slot_data|to_raw_value\\(data\\)|from_str\\(data\\.get\\(\\)\\)" lp-core/lpc-wire lp-core/lpc-engine lp-core/lpc-view
```

The `rg` command should produce no matches.
