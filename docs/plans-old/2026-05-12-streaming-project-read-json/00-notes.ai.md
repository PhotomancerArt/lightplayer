# Notes: Streaming Project Read JSON

## Scope

Build a bounded-memory JSON writing path for project read responses, with enough tests to prove correctness before wiring it into ESP transport.

The plan should improve the memory shape without forcing a full message protocol rewrite:

- Keep JSON as the wire format for now.
- Keep semantic inner structs such as `ProjectReadResult`, `NodeReadResult`, `ResourceReadResult`, and `ProjectProbeResult` where they are useful.
- Avoid constructing or serializing a whole `ProjectReadResponse` buffer on ESP.
- Stream the project-read response envelope and write results/probes one at a time.
- Add a semantic JSON writer so call sites do not manually manage commas.
- Leave a path for specialized streaming of heavy fields, especially resource payload bytes and slot snapshots.

## Current Code Context

### Response Construction

- `lp-core/lpc-engine/src/engine/project_read.rs`
  - `Engine::read_project(request) -> ProjectReadResponse` currently builds the full response object.
  - It collects all query results into `Vec<ProjectReadResult>` and all probe results into `Vec<ProjectProbeResult>`.
  - This is simple and good for host/tests, but it means embedded owns the entire response before serialization.

### Response Shape

- `lp-core/lpc-wire/src/messages/project_read/project_read_response.rs`
  - `ProjectReadResponse { revision, results: Vec<ProjectReadResult>, probes: Vec<ProjectProbeResult> }`.
  - `ProjectReadResult` has `Shapes`, `Nodes`, and `Resources` variants.
  - The JSON envelope is simple enough to stream by hand while keeping inner result structs.

### Slot Sync Shape

- `lp-core/lpc-wire/src/slot/sync.rs`
  - `WireSlotFullSync { registry, roots }`.
  - `WireSlotRootSnapshot { name, shape, data }`.
  - This can become heavy because slot roots and shape registries are cloned into owned sync structs.

- `lp-core/lpc-wire/src/slot/access_sync.rs`
  - `snapshot_slot_root` recursively builds owned `SlotData`.
  - `build_slot_full_sync` also snapshots the whole registry.
  - First version of this plan does not need to stream individual slot roots, but this is an obvious future specialization.

### Resource Payload Shape

- `lp-core/lpc-wire/src/project/resource_sync.rs`
  - `WireRuntimeBufferPayload { resource_ref, revision, metadata, bytes: Vec<u8> }`.
  - The bytes currently serialize with `crate::serde_base64`, which will usually require handling the complete byte vector and may create encoded temporary data depending on the serializer path.
  - Resource payloads are the most obvious candidate for a specialized streaming writer.

### ESP Serial Path

- `lp-fw/fw-esp32/src/serial/io_task.rs`
  - Comments claim server messages are serialized directly to serial and never buffer full JSON.
  - Actual code uses `VecWriter` and `ser_write_json::ser::to_writer(&mut VecWriter(&mut buf), &msg)`.
  - That builds a full serialized JSON `Vec<u8>`, then `timed_write_all` chunks that vector to serial.
  - This is not truly streaming and can OOM on large responses.

### JSON Facade

- `lp-core/lpc-wire/src/json.rs`
  - Currently a small facade over `serde_json`.
  - Earlier code used `serde-json-core` with a capped buffer because of an ESP linker/boot issue. The boot issue is now believed fixed, but heap pressure remains.
  - The facade is useful as a stable import path, but it does not solve streaming.

### Debug UI Mitigations Already Added

- `lp-cli/src/debug_ui/ui.rs`
  - Debug UI now sends `since = view.revision`.
  - It only asks for a full node slot snapshot when the client has no roots.
  - It requests resource payloads only for the selected resource.
  - This reduces pressure but does not solve the underlying transport/serialization issue.

## User Notes To Preserve

- The problem is not going away; building more state and nodes on top of buffered JSON may make it harder to back out.
- The user has not been convinced serde-buffered JSON is the right way to stream messages.
- A stream writer API that flushes directly to serial may be the right approach under tight memory constraints.
- The on-wire format matters less than memory footprint right now.
- Good compromise idea:
  - Keep inner objects like `ProjectReadResult` and `ProjectProbeResult`.
  - Stop writing/constructing the full `ProjectReadResponse` envelope as one owned object on ESP.
  - Hardcode/generate the response JSON envelope with a streaming API.
  - Serialize each inner result/probe to the stream and discard it before generating the next.
  - Add special handling for resources so binary data is not double/triple buffered.
- Desired JSON writer API should be semantic, not manual comma writing:
  - `writer.object()` writes `{` and tracks commas.
  - `object.prop("name")` writes a property name and colon with correct comma handling.
  - `value.num(15)` or similar writes the value.

## Open Questions

### Q1. Should the first implementation be JSON-specific or protocol-generic?

Context: The user is less worried about JSON itself than memory footprint, but a tiny semantic JSON writer is the natural first step.

Suggested answer: Make it JSON-specific for now, under `lpc-wire`, because this solves the active ESP memory issue while keeping protocol semantics stable. Design the API so a later binary/message writer could reuse the higher-level project-read streaming shape.

Status: Confirmed. Build JSON-specific first; keep later protocol backends possible.

### Q2. Should ESP still carry full `WireServerMessage` through the outgoing channel?

Context: `StreamingMessageRouterTransport::send` currently sends a full `WireServerMessage` through `OUTGOING_SERVER_MSG`. Even if serialization becomes streaming, the server has already built a full response if the normal `Engine::read_project` path is used.

Suggested answer: For this plan, add a new project-read-specific streaming path that bypasses building `ProjectReadResponse` for ESP. Do not redesign all server message channels yet. Keep normal `WireServerMessage` for small responses/heartbeat and host paths.

Status: Confirmed. Add a project-read streaming path for ESP while preserving normal host response structs.

### Q3. How specialized should resource payload streaming be in this plan?

Context: Resource payload bytes are the most dangerous field because base64 encoding can allocate another large buffer or require the whole payload in memory.

Suggested answer: Include a base64 streaming helper and use it for `WireRuntimeBufferPayload` when writing resource results through the streaming project-read writer. Do not attempt to stream every slot-data variant in this plan.

Status: Confirmed. Include streaming base64 for runtime-buffer/resource payload bytes.

### Q4. Should host clients continue to deserialize the same JSON shape?

Context: The lowest-risk design keeps the same JSON document shape so `ProjectReadResponse` can still be deserialized normally by clients.

Suggested answer: Yes. Streamed output must round-trip through normal `serde_json::from_str::<ProjectReadResponse>` in tests.

Status: Confirmed. Streamed JSON must preserve the existing deserializable response shape.

## Non-Goals

- Do not design a binary protocol in this plan.
- Do not remove serde from the host/client paths.
- Do not rewrite all message types.
- Do not solve incremental slot diffing or slot-root streaming unless needed for the envelope writer tests.
- Do not disable the on-device compiler or weaken ESP features to make memory look better.
