# Project Read End-To-End Streaming Summary

## What Was Built

- Added direct JSON writers for project-read responses and server-message envelopes, with serde-deserialization tests to guard wire-shape drift.
- Added a `ProjectReadJsonSource` / `ServerTransport::send_project_read` boundary so `lpa-server` can route project reads without forcing every transport through a full semantic response object.
- Added `LpServer::tick_and_send` and switched the desktop and ESP server loops to the streaming-aware path.
- Added an ESP32 raw JSON chunk channel and chunked project-read writer so firmware project reads avoid a full `WireServerMessage`/JSON-frame heap allocation.
- Split roots-only slot snapshots from full slot sync snapshots so node reads no longer smuggle an optional shape registry.
- Added direct slot-shape-registry JSON writing for shape reads, avoiding a registry clone on the streamed engine path.
- Kept non-allocating ESP32 OOM diagnostics and fixed schema-gen coverage for new resource/probe wire payloads.

## Decisions For Future Reference

#### Project Read Is Transport-Streamable

- **Decision:** Project reads now have a dedicated transport hook instead of always using `send(WireServerMessage)`.
- **Why:** Project reads can be much larger than normal responses and need a firmware path that does not allocate the whole response.
- **Rejected alternatives:** Making all messages streaming immediately; replacing JSON now.
- **Revisit when:** Other response domains become large enough to need the same treatment.

#### Firmware Uses Fixed Raw Chunks

- **Decision:** ESP32 project-read JSON is sent through a fixed-size raw chunk channel owned by `io_task`.
- **Why:** The serial sink is async while the JSON writer is sync; fixed chunks give us bounded heap behavior without a full async JSON rewrite.
- **Rejected alternatives:** Sending a full `WireServerMessage`; duplicating project-read JSON by hand in firmware.
- **Revisit when:** We add async direct writers or a lower-level serial sink that can be borrowed by the server loop.

#### Shape Registry Streaming Starts At The Registry

- **Decision:** Shape reads write the live `SlotShapeRegistry` directly instead of cloning a `SlotShapeRegistrySnapshot` first.
- **Why:** Shape detail is one of the large debug-read payloads and was a likely contributor to ESP32 memory spikes.
- **Rejected alternatives:** Treating shape snapshots as always-small; making the UI avoid shapes entirely.
- **Revisit when:** We add registry diffs or shape-level LOD.

#### Full Slot Sync And Roots-Only Node Reads Are Separate

- **Decision:** `WireSlotFullSync` requires a registry, while `WireSlotRootsSnapshot` is used when the registry is already synced separately.
- **Why:** Optional registry fields made the type lie about what it represented and confused client application logic.
- **Rejected alternatives:** Keeping `registry: Option<_>` for compatibility.
- **Revisit when:** Slot watches/diffs replace full roots in steady-state reads.
