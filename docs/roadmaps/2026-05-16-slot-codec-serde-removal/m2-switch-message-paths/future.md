# Future Work

## Typed Client Slot Payloads

- **Idea:** Add a client-side project-read representation that keeps slot root
  payloads as registry-readable SlotCodec payloads instead of `SlotData`.
- **Why not now:** M2 only needs to prove the first real message payload path;
  changing public client return types would expand the review surface.
- **Useful context:** `lpa-client::Client::project_read` currently returns
  `ProjectReadResponse`.

## Shape Snapshot Writer Without Serde

- **Idea:** Replace `write_slot_shape_registry_snapshot_json` serde bridges for
  shape entries with a slot/schema-native writer.
- **Why not now:** M2 focuses on model slot data payloads; schema snapshots are
  part of the final serde-removal milestone.
- **Useful context:** `lpc-wire/src/slot/slot_shape_registry_json.rs`.

## Desktop Transport Raw Project-Read Frames

- **Idea:** Let desktop/default transports send already-written project-read
  server-message JSON without deserializing it back into `ProjectReadResponse`.
- **Why not now:** the local desktop transport is typed around
  `WireServerMessage`, so avoiding the fallback serde round-trip needs a
  broader transport/client boundary decision rather than a tiny hook.
- **Useful context:** ESP32 already overrides `send_project_read` with a
  streaming path. The default fallback in
  `lpc-shared/src/transport/server.rs` remains a temporary desktop bridge.
