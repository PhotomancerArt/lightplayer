# Project Read End-To-End Streaming Design

## Scope Of Work

Make large server responses write directly to transport sinks instead of first
building large semantic response objects and then serializing them.

The first production target is ESP32 project-read OOM, but the design applies
to all potentially large responses:

- project-read `results` and `probes`
- shape registries
- node/tree/slot root arrays
- resource summaries and payloads
- filesystem read payloads

Desktop-class transports may still collect into memory for convenience. Firmware
transports must use bounded direct streaming.

## File Structure

```text
lp-core/
  lpc-shared/src/transport/
    server.rs

  lpc-wire/src/
    json/
      json_write.rs
      json_writer.rs
      streaming_base64.rs
    messages/
      project_read/
        stream_response.rs
      stream_server_message.rs        # new
    server/
      stream_fs_response.rs           # new or equivalent location
    slot/
      sync.rs

  lpc-engine/src/engine/
    project_read.rs
    project_read_stream.rs
    project_read_nodes.rs
    project_read_shapes.rs

  lpc-view/src/
    project/apply_project_read.rs
    slot/mirror.rs

lp-app/
  lpa-server/src/
    handlers.rs
    server.rs

lp-fw/
  fw-esp32/src/
    transport.rs
    serial/io_task.rs
    main.rs

lp-cli/src/
  server/run_server_loop_async.rs
  debug_ui/ui.rs
```

## Architecture Summary

The current path still materializes project-read responses:

```text
ClientMessage
  -> lpa-server handlers
  -> Engine::read_project()
  -> ProjectReadResponse { Vec<ProjectReadResult>, Vec<ProjectProbeResult> }
  -> WireServerMessage
  -> transport
  -> JSON writer
```

The new path should stream large responses from the producer:

```text
ClientMessage
  -> streaming-aware server handling
  -> transport.send_project_read(id, &Engine, ProjectReadRequest)
  -> lpc-wire server envelope writer
  -> Engine::write_project_read_json(...)
  -> transport sink
```

Normal small messages may still use:

```text
WireServerMessage -> transport.send(...)
```

The canonical wire shape remains unchanged. Clients still receive and deserialize
the same JSON envelope:

```json
{
  "id": 1,
  "msg": {
    "projectRequest": {
      "response": {
        "revision": 6,
        "results": [],
        "probes": []
      }
    }
  }
}
```

## Main Components

### Direct Writers In `lpc-wire`

`lpc-wire` owns the canonical direct JSON writers so ESP does not hand-author
project JSON in firmware modules.

Writers should cover:

- server message envelope for project-read responses
- project-read inner response/results/probes
- resource payload bytes
- filesystem read payload bytes

Every direct writer must have tests that deserialize the produced JSON through
the normal serde wire type and compare against expected semantic values.

### Streaming Transport API

`ServerTransport` gains explicit large-response streaming methods, such as:

```rust
async fn send_project_read(
    &mut self,
    id: u64,
    engine: &lpc_engine::Engine,
    request: lpc_wire::ProjectReadRequest,
) -> Result<(), TransportError>;
```

Exact type placement may change to avoid making `lpc-shared` depend on
`lpc-engine`. If that dependency is wrong, use a callback/trait writer boundary
instead:

```rust
async fn send_streamed(
    &mut self,
    id: u64,
    stream: impl FnOnce(JsonWriter<...>) -> ...
) -> ...
```

The implementation should preserve the important invariant: firmware receives
a writer/sink and does not need a full `ProjectReadResponse`.

Desktop transports may implement streaming by collecting bytes into a `Vec` and
then sending the string/message. Firmware transports should write bounded chunks
directly.

### Server Routing

`LpServer::tick` returning `Vec<WireMessage>` is insufficient for true
streaming. Add a streaming-aware path for server loops:

```rust
server.tick_and_send(delta_ms, incoming_messages, &mut transport).await
```

or equivalent.

This method should:

- perform filesystem-change processing
- tick projects
- route normal messages through existing handlers
- route `ProjectRequest::Read` through the transport streaming method
- send errors as normal messages

The old `tick(...) -> Vec<WireMessage>` can remain for tests and simple host
callers, but firmware should use the streaming-aware method.

### ESP Serial Writer

ESP serial should become a thin bounded sink for `lpc-wire` writers.

Keep:

- chunked writes
- write timeouts
- non-allocating OOM diagnostics

Remove or reduce:

- ESP-local duplicated project-read JSON writer
- heap buffering of large messages
- debug `esp_println` calls inside raw `M!` frames

### Shape And Slot Sync Cleanup

`NodeReadResult.slots` should not carry a full shape registry when the project
read response also has `ProjectReadResult::Shapes`.

Clean up the WIP optional registry by introducing separate types:

```rust
WireSlotFullSync {
    registry: SlotShapeRegistrySnapshot,
    roots: Vec<WireSlotRootSnapshot>,
}

WireSlotRootsSnapshot {
    roots: Vec<WireSlotRootSnapshot>,
}
```

Use `WireSlotRootsSnapshot` for `NodeReadResult.slots`.

### Debug UI Staging

The debug UI can keep staged reads:

- first read shapes when the local shape registry is empty
- read detailed node slots only when slot roots are missing or explicitly needed
- steady-state polls can use summaries/resources without large slot roots

This is a bandwidth and UX improvement, not a substitute for server-side
streaming.

## Non-Goals

- Do not change client-visible JSON shape unless unavoidable.
- Do not replace JSON in this plan.
- Do not redesign resource ownership.
- Do not solve registry diffs yet.
- Do not remove the normal `read_project` semantic API until tests and host code
  no longer benefit from it.

