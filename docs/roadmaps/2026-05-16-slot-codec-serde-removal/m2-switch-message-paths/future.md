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
