# M2 Switch Message Paths Notes

## Scope

Milestone 2 switches the first real JSON/message path from Serde-owned model
payloads to SlotCodec-owned model payloads.

Chosen slice:

- project-read node slot payloads
- server writes slot root `data` through `SlotShapeRegistry::write_slot_json_value`
- client/test parsing of that slot root `data` should use slot shape metadata
  and SlotCodec, not `serde_json` into `SlotData`

Out of scope:

- removing Serde derives from `lpc-model`
- removing Serde from non-slot envelope fields such as `id`, `revision`, or
  `ReadLevel`
- switching authored TOML definition loading
- redesigning public project-read envelopes beyond what is needed for slot
  payloads

## Current State

The project-read path already has a partial direct writer:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
  - `Engine::write_project_read_json`
  - `write_project_node_read_result_json`
  - `write_slot_data_json_value`
- `write_slot_data_json_value` already writes borrowed slot data through:
  `self.slot_shapes().write_slot_json_value(root.shape_id(), root.data(), writer.value())`

But the verification path still deserializes the full streamed response through
Serde:

- `ProjectReadResponse = lpc_wire::json::from_slice(&streamed)`
- `ProjectReadResponse` contains `NodeReadResult`
- `NodeReadResult.slots` contains `WireSlotRootsSnapshot`
- `WireSlotRootSnapshot.data` is `lpc_model::SlotData`
- `SlotData` currently derives Serde and is the old in-memory tree snapshot

The default desktop transport also still falls back through this full serde
round trip:

- `lp-core/lpc-shared/src/transport/server.rs`
  - `ServerTransport::send_project_read`
  - writes project-read JSON to `Vec<u8>`
  - deserializes it back into `ProjectReadResponse`
  - sends a normal `WireServerMessage`

The ESP32 transport has a streaming override:

- `lp-fw/fw-esp32/src/transport.rs`
  - `send_project_read`
  - writes a project-read server message as JSON chunks

That override is closer to the intended embedded path.

## Why Project Read Slots First

Mutation requests are smaller, but `WireSlotMutationOp::SetValue(LpValue)` is
not registry-shaped; it can mostly exercise `LpValue` parsing. Project-read node
slots exercise the real slot model:

- root names
- `SlotShapeId`
- shape registry snapshots
- structured records
- maps
- `EnumSlot<T>` payloads
- semantic leaves
- direct JSON writer output

This makes project-read slots the better M2 proof even though the surrounding
response envelope still uses Serde for now.

## Relevant Files

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_shapes.rs`
- `lp-core/lpc-shared/src/transport/server.rs`
- `lp-core/lpc-wire/src/messages/project_read/node_read.rs`
- `lp-core/lpc-wire/src/messages/project_read/shape_read.rs`
- `lp-core/lpc-wire/src/messages/project_read/stream_response.rs`
- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`

## Open Questions

### Q1. Should M2 change the public typed `ProjectReadResponse` model?

Context: `ProjectReadResponse` currently contains `SlotData`, which forces
Serde to build an in-memory tree when tests or default transports parse the
response. A full public type redesign would be bigger than M2.

Suggested answer: no. Keep the typed response structs for now, but add
slot-codec-specific readers/writers for the slot roots inside the project-read
JSON. Tests should inspect/round-trip slot roots through the registry rather
than relying on `ProjectReadResponse` serde equality for detailed node slots.

### Q2. Should the default desktop `ServerTransport::send_project_read` stop
round-tripping through `ProjectReadResponse` in M2?

Context: the fallback defeats streaming by writing JSON, deserializing it, then
serializing again through `send`. Changing the trait to support raw JSON frames
would affect desktop transports.

Suggested answer: include a small transport extension if it stays contained:
add an optional `send_json_bytes`/`send_server_json` style hook or equivalent
only if implementation proves straightforward. Otherwise, document this as a
follow-up and keep M2 focused on project-read slot payloads.

### Q3. Should shape registry snapshots move off Serde in M2?

Context: project-read shape results still use
`write_slot_shape_registry_snapshot_json`, but that writer serializes individual
shape entries through the serde bridge.

Suggested answer: no. Shape snapshot serde removal belongs closer to M4. M2 is
about model slot data payloads, not schema snapshot metadata.

## User Notes

- Use a switch-it-and-fix-it approach.
- Leave serde annotations and helpers in place for now.
- Switch core behavior one path at a time, fix tests, then repeat.
- Start with messages; definitions follow in M3.
