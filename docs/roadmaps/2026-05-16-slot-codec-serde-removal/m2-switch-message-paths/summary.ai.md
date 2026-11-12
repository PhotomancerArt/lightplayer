# Summary

## What was built

- Project-read detailed node slot payloads are now tested by reading the emitted
  JSON `data` payloads back through `SlotShapeRegistry::read_slot_json`.
- `JsonValue` implements `SlotWrite`, so SlotCodec can write structured slot
  payloads directly into the project-read JSON envelope without a per-root
  temporary `Vec<u8>`.
- The dummy shader runtime state test fixture now registers a default factory,
  allowing registry-backed dynamic reads to create and hydrate the emitted
  runtime state root.
- The desktop transport raw-frame issue is recorded as future work because the
  current local/default transport boundary is typed around `WireServerMessage`.

## Decisions for future reference

#### Direct Slot Payload Writer

- **Decision:** adapt `JsonValue` to `SlotWrite`.
- **Why:** the two writer traits have the same byte-sink shape, and this keeps
  structured slot payloads streaming without introducing `SlotData` or a JSON
  tree.
- **Rejected alternatives:** keep per-root `Vec<u8>` bridge; introduce a generic
  JSON value tree.

#### Transport Fallback Deferred

- **Decision:** leave the default desktop `send_project_read` fallback in place
  for M2.
- **Why:** avoiding its serde round-trip requires a broader raw-frame transport
  path, especially for the local typed client/server channel.
- **Rejected alternatives:** widen M2 into a transport refactor; weaken the ESP32
  streaming override.
