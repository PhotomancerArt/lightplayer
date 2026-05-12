# Future Work

## Binary Or Framed Protocol

- **Idea:** Replace JSON with a compact framed protocol for embedded clients.
- **Why not now:** The immediate issue is peak memory, and streamed JSON can fix that without redesigning every client/server message.
- **Useful context:** Keep the high-level project-read sink API protocol-neutral enough that a future binary writer could share the same engine generation path.

## Stream Slot Roots Internally

- **Idea:** Stream `WireSlotFullSync` roots and large `SlotData` subtrees without first building full owned snapshots.
- **Why not now:** The first win is response-envelope and resource-payload streaming; slot streaming requires more careful interaction with `SlotAccess` and shape traversal.
- **Useful context:** `lp-core/lpc-wire/src/slot/access_sync.rs` currently snapshots recursively into owned `SlotData`.

## Server-Side Project Read Sink

- **Idea:** Route project-read requests through `Engine::write_project_read_json` before constructing a full `ProjectReadResponse`.
- **Why not now:** The current server tick API returns owned `WireMessage`s for every response. This plan removed the full serialized JSON buffer on ESP first; changing server tick to accept a response sink is a larger ownership/API pass.
- **Useful context:** `lpa-server` still calls `project.engine().read_project(request)` in the normal handler path, so semantic response objects can still exist before serial streaming.

## Chunked Resource Reads

- **Idea:** Add explicit chunk/range reads for resource payloads instead of sending whole resources when selected.
- **Why not now:** Streaming base64 removes duplicate buffers, but a very large resource can still be too much to send as one response.
- **Useful context:** `ResourcePayloadRead::ByRefs` selects whole resources today.
