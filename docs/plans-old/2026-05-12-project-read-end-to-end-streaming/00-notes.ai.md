# Project Read End-To-End Streaming Notes

## Scope Of Work

Make project-read responses stream all the way from the engine/server handling
path to the transport writer, instead of first building a full
`ProjectReadResponse` and then streaming only the JSON serialization.

The immediate trigger is ESP32 OOM during the debug UI project read. Profiling
and device traces show the current path still allocates heavily while building
and writing project-read results, especially node slot roots and shape data.

In scope:

- Add an end-to-end streamed project-read response path.
- Add direct writer paths for any response payload that can plausibly become
  large, not only project reads.
- Preserve the current JSON wire shape for `ProjectRequest { response: ... }`
  so existing clients can continue to parse `ProjectReadResponse`.
- Let transports stream a project read directly when they can, while keeping a
  reasonable fallback for host/local transports.
- Wire `lpa-server` so `ProjectRequest::Read` can be handled without allocating
  a full `ProjectReadResponse`.
- Use `Engine::write_project_read_json` or its successor as the core streaming
  producer.
- Remove or contain ESP-only ad hoc project-read JSON duplication once the
  generic streaming path is wired.
- Keep resource payload bytes streamed/base64 encoded without duplicate buffers.
- Stream large top-level arrays and byte payloads directly, including project
  read `results`/`probes`, shape registries, node roots, resource lists, and
  filesystem read bytes.
- Add tests that prove the streamed response is byte-for-byte or semantically
  equivalent to the current full response.
- Add serde-deserialization tests for direct-written JSON so format drift is
  caught whenever a hand-written writer changes.
- Add memory-oriented instrumentation only where it does not corrupt `M!` JSON
  frames.

Out of scope:

- Redesigning the project sync protocol beyond what streaming needs.
- Rebuilding the real UI.
- Client-driven mutation.
- Registry diff protocol.
- Replacing JSON or changing the outer message envelope format.
- Solving all heap fragmentation issues across the runtime.

## Current Code State

### Engine

- `lp-core/lpc-engine/src/engine/project_read.rs` builds a full
  `ProjectReadResponse`:
  - `revision`
  - `Vec<ProjectReadResult>`
  - `Vec<ProjectProbeResult>`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs` already has
  `Engine::write_project_read_json(request, out)`.
- `Engine::write_project_read_json` uses `ProjectReadResponseWriter`, but it
  still constructs each `ProjectReadResult` before writing it.
- The current stream method avoids the full response envelope allocation, but
  it is not used by the server/ESP path.

### Wire

- `lp-core/lpc-wire/src/messages/project_read/stream_response.rs` defines
  `ProjectReadResponseWriter<W>`.
- `ProjectReadResponseWriter` emits the inner response object:
  `{ "revision": ..., "results": [...], "probes": [...] }`.
- `lpc-wire` JSON helpers include:
  - `JsonWrite`
  - `JsonWriter`
  - streaming base64 helpers for runtime buffer payloads.
- The direct writers must be tested by deserializing their output with serde
  into the normal Rust wire types. This is important because hand-written JSON
  has format-drift risk.
- The full server envelope type is still:
  `WireServerMessage = ServerMessage<ProjectReadResponse>`.
- `ServerMsgBody<R>::ProjectRequest { response: R }` is generic, which gives
  us room to keep the semantic envelope while streaming project-read manually.

### Server

- `lp-app/lpa-server/src/handlers.rs::handle_project_request` currently does:
  `project.engine().read_project(request)`.
- `handle_client_message` always returns `WireServerMessage`.
- `LpServer::tick` always returns `Vec<WireMessage>`.
- This means the project-read response is fully allocated before the transport
  sees it.
- Host and ESP loops both call `server.tick(...)`, then send returned messages.

### Shared Transport

- `lp-core/lpc-shared/src/transport/server.rs::ServerTransport` only has:
  - `send(WireServerMessage)`
  - `receive`
  - `receive_all`
  - `close`
- There is no API for a server to ask the transport for a JSON sink or stream a
  project read directly.
- All transports currently receive a fully materialized `WireServerMessage`.

### ESP Transport And Serial Writer

- `lp-fw/fw-esp32/src/transport.rs::StreamingMessageRouterTransport` sends a
  full `WireServerMessage` through a capacity-1 channel to `io_task`.
- `lp-fw/fw-esp32/src/serial/io_task.rs` receives that message and serializes it.
- Recent WIP changed non-project messages to serialize into a fixed stack
  buffer rather than a heap `Vec`.
- Recent WIP added custom project-read JSON writing in `io_task`, but that
  still receives a fully allocated `ProjectReadResponse`.
- ESP `io_task` currently has duplicated project-read/resource JSON writing
  logic that overlaps with `lpc-wire`.

### View And Debug UI

- `lp-cli/src/debug_ui/ui.rs` currently polls project reads and applies the
  returned `ProjectReadResponse` to `lpc-view`.
- Recent WIP makes the UI request shapes only when it needs the initial slot
  snapshot.
- `lpc-view` expects a complete `ProjectReadResponse` after client parsing.
  That can stay as-is because streaming is server-side serialization only; the
  client can still deserialize the same JSON shape.

### Current WIP In Working Tree

At plan creation time there are uncommitted changes in:

- `lp-core/lpc-model/src/slot/slot_data.rs`
  - `SlotMapDyn` entries serialize as key/data arrays to avoid invalid JSON
    object keys.
- `lp-fw/fw-esp32/src/main.rs`
  - OOM handler prints allocation failure info without allocating more payload
    data.
- `lp-fw/fw-esp32/src/serial/io_task.rs`
  - stack JSON writer and partial project-read streaming writer.
- `lp-core/lpc-wire/src/slot/sync.rs`,
  `lp-core/lpc-wire/src/slot/access_sync.rs`,
  `lp-core/lpc-engine/src/engine/project_read_nodes.rs`,
  `lp-core/lpc-view/src/slot/mirror.rs`,
  `lp-cli/src/debug_ui/ui.rs`
  - WIP split so `NodeReadResult.slots` can omit the shape registry and shapes
    can be read through `ProjectReadQuery::Shapes`.
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
  - WIP `is_empty` helper added during interrupted exploration.

These changes should be reviewed in phase 0. Keep the pieces that are still
needed, revert/debug-clean anything that is superseded by the end-to-end stream
path, and avoid committing accidental instrumentation.

## Evidence And Diagnosis

Device trace `target/2026-05-11-2031-oom-dump.txt` showed:

- Before server tick with incoming request: about `159k` free.
- After server tick responses built: about `135k` free.
- During first project-read result write, the response included a very large
  node slot payload with shape registry data and roots.
- After first result write: about `85k` free.
- Later OOM requested `2048` bytes while only about `14k` free remained.

Interpretation:

- The JSON writer reduced serialization buffering, but did not prevent full
  `ProjectReadResponse` construction.
- The ESP path still held full semantic response data while writing it.
- Shape registry duplication inside node slot sync was an additional pressure
  source, but not the complete architectural fix.
- Debug breadcrumbs written with `esp_println` during an `M!` frame corrupt the
  JSON stream and should not be used inside raw message writes.
- The same memory failure pattern can recur anywhere we serialize large
  responses by first building a full JSON buffer or a full semantic response
  object. Filesystem reads, project root arrays, shape maps, node arrays, slot
  roots, resource summaries, and payload arrays should all use direct writers
  when they can be large.

## Open Questions

### Q1. Should Streaming Be Added To `ServerTransport` Or Kept ESP-Specific?

Context: the actual bug is cross-layer: `LpServer::tick` returns a full
response vector before transport begins sending. Adding an ESP-only channel for
streaming project reads may fix the device faster, but keeps the server
architecture split-brained.

Suggested answer: add a generic transport method for streamed project reads or
streamed server responses, with a default fallback that builds the full response
for simple transports. ESP implements the true streaming path.

User answer: yes. Desktop-class servers such as the `lp-cli` server may stream
into a `String`/`Vec` or otherwise keep the simpler path. Firmware targets
should use true streaming.

### Q2. What Should The Server Return Type Be?

Context: `LpServer::tick` currently returns `Vec<WireMessage>`. A streaming
project read cannot be represented as an already-built `WireMessage`.

Suggested answer: introduce a server output enum, for example:

```rust
pub enum ServerOutput {
    Message(WireServerMessage),
    ProjectRead {
        id: u64,
        handle: WireProjectHandle,
        request: ProjectReadRequest,
    },
}
```

The server loop sends each output through the transport. The transport can
stream `ProjectRead` by calling back into `LpServer`/`ProjectManager` or by
receiving a borrowed engine writer closure.

Concern: this enum cannot own a borrow of the engine. The implementation needs
to avoid storing borrowed project references across `await`.

User answer: probably fine, but design should keep the borrow/lifetime issue
clear.

### Q3. Where Should The Borrow Live During Streaming?

Context: true streaming needs access to `project.engine()` while writing JSON.
But transport `send(...).await` is async, and borrowing `server` across await is
often awkward. ESP serial writing is async; host WebSocket send is async.

Suggested answer: add a server method like:

```rust
async fn handle_client_message_streaming<T: ServerTransport>(
    &mut self,
    transport: &mut T,
    client_msg: ClientMessage,
    delta_context...
) -> Result<(), ServerError>
```

This method can route normal messages through `transport.send(...)` and route
project reads through a new `transport.send_project_read(id, engine, request)`
method while borrowing the engine only for the duration of the awaited call.

This may require an async trait method that accepts `&Engine`; that is workable
because the borrow is local to the await, but it should be validated carefully.

### Q4. Should `Engine::write_project_read_json` Emit The Inner Response Or The
Full Server Envelope?

Context: it currently emits only the inner `ProjectReadResponse` object. ESP
serial code manually writes:
`M!{"id":...,"msg":{"projectRequest":{"response": ... }}}\n`.

Suggested answer: keep `Engine::write_project_read_json` focused on the inner
response object. Put the server envelope writer in `lpc-wire`, not in ESP, so
all transports can reuse one canonical writer.

User answer: probably.

### Q5. Should Project-Read Result Writers Avoid Allocating Individual Results?

Context: `Engine::write_project_read_json` currently creates each
`ProjectReadResult`, then writes it. That still allocates per query, but not the
whole response.

Suggested answer: phase 1 wires end-to-end streaming using the existing engine
writer. Phase 2 improves the engine writer to stream heavy query results
directly, especially:

- shape registry result
- node slot roots
- resource summaries and payloads

This keeps the plan incremental and lets us measure after each cut.

User answer: agreed; deeper individual result streaming can come later.

### Q6. What Happens To The Shape Split WIP?

Context: `NodeReadResult.slots` carrying a full registry duplicates
`ProjectReadResult::Shapes` and bloats the first detail read.

Suggested answer: keep the split, but clean up the naming. Prefer replacing
`WireSlotFullSync { registry: Option<_>, roots }` with two types:

```rust
WireSlotFullSync { registry, roots }
WireSlotRootsSnapshot { roots }
```

Then `NodeReadResult.slots` should use `WireSlotRootsSnapshot`.

### Q7. How Should Memory Debugging Be Kept?

Context: OOM diagnostics in `main.rs` are helpful and do not corrupt JSON. Heap
breadcrumbs emitted with `esp_println` inside message writes corrupt `M!`
frames.

Suggested answer: keep non-allocating OOM diagnostics. Add optional, guarded
memory logs only at frame/message boundaries before a raw JSON frame starts or
after it finishes. Do not print during a streamed `M!` frame.

User answer: yes.

### Q8. Should The Debug UI Split Requests Anyway?

Context: even with true streaming, asking for shapes, node detail, roots, and
resources in one response can be large on low-memory devices. The UI can stage
requests without changing protocol semantics.

Suggested answer: keep staged debug UI reads as a complementary improvement:
initial shapes, then node slots, then steady-state summaries/resources. But do
not rely on this as the primary memory fix.

### Q9. Should Direct Writers Cover Filesystem Reads Too?

Context: `FsResponse::Read` can contain arbitrary file bytes. Even if most
project files are small today, this is the same class of issue as resource
payloads: byte data should not be duplicated into multiple buffers while
writing a response.

Suggested answer: yes. Add direct writer support for filesystem read responses,
especially binary or large text payloads. The implementation can still use
serde for small filesystem metadata fields, but the data bytes should be
streamed/base64 or otherwise written without a duplicate intermediate buffer.

### Q10. How Do We Guard Hand-Written JSON Against Format Drift?

Context: direct writers are memory-friendly but carry drift risk because serde
derives remain the canonical shape used by clients.

Suggested answer: every direct writer introduced in this plan needs a test that
serializes through the direct writer, deserializes using the normal serde-based
wire type, and compares to the original semantic object. For stream-only paths
that avoid constructing the full semantic object, use representative fixtures
and assert the deserialized output matches the expected wire object.
