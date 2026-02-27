# Design: Streaming Outgoing Transport

## Scope of Work

1. **Streaming outgoing messages on ESP32**: Replace chunked channel approach with serialize-in-io-task. Pass `ServerMessage` through `Channel<ServerMessage, 1>`; io_task receives and serializes directly to serial with ser-write-json. Never buffer full JSON.

2. **Async ServerTransport**: Make `ServerTransport` fully async (send, receive, close) per 2026-02-04-async-server-transport plan. Enables blocking backpressure via `await` on channel send.

3. **Cleanup uncommitted changes**: Keep useful lp-model changes (ser-write-json tests, SerializableNodeDetail format). Remove ChunkingSerWrite and OUTGOING_CHUNKS. Adapt test_json to new transport.

## File Structure

```
lp-core/lp-shared/src/transport/
├── server.rs                    # UPDATE: async fn send, receive, close

lp-core/lp-model/
├── Cargo.toml                   # KEEP: ser-write-json feature (uncommitted)
├── src/
│   ├── json.rs                  # KEEP: ser_write_json_tests (uncommitted)
│   └── project/
│       └── api.rs               # KEEP: NodeStateSerializer, serialize_struct_variant (uncommitted)

lp-fw/fw-esp32/src/
├── main.rs                      # UPDATE: use new transport, test_json gate
├── transport.rs                 # REWRITE: StreamingMessageRouterTransport
│                                #         - Channel<ServerMessage, 1>
│                                #         - async send awaits channel.send()
├── serial/
│   └── io_task.rs               # UPDATE: OUTGOING_SERVER_MSG, serialize in task
├── server_loop.rs               # UPDATE: transport.send().await, receive().await
└── tests/
    └── test_json.rs             # UPDATE: use new transport architecture

lp-fw/fw-core/src/transport/
├── message_router.rs            # UPDATE: async ServerTransport (or remove if unused)
├── serial.rs                    # UPDATE: async ServerTransport
└── fake.rs                      # UPDATE: async ServerTransport

lp-fw/fw-emu/src/
└── server_loop.rs               # UPDATE: block_on for async transport

lp-core/lp-client/src/
└── local.rs                     # UPDATE: AsyncLocalServerTransport async

lp-cli/src/server/
└── transport_ws.rs              # UPDATE: WebSocketServerTransport async

justfile                           # KEEP: fwtest-json-esp32c6 (uncommitted)
```

## Conceptual Architecture

### Current (Chunked - To Be Replaced)

```
Server Loop                    IO Task
    |                              |
    | send(msg)                    |
    v                              |
StreamingMessageRouterTransport   |
    |                              |
    | to_writer -> ChunkingSerWrite|
    |   -> Channel<Vec<u8>,256>   |
    |                              | try_receive chunk
    |                              v
    |                         Write to serial
    |                              ^
    |   (128KB can queue!)         |
```

### New (Serialize in IO Task)

```
Server Loop                    IO Task
    |                              |
    | send(msg).await              |
    v                              |
StreamingMessageRouterTransport   |
    |                              |
    | channel.send(msg).await      |
    |   (blocks if full)           |
    v                              v
Channel<ServerMessage, 1>    try_receive(msg)
    |                              |
    |                              | to_writer(&msg, SerialWriter)
    |                              |   (streams to serial, small buffer)
    |                              v
    |                         Write to serial
    |                              |
    +------------------------------+
         (max 1 msg in flight)
```

### SerWrite for Direct Serial

The io_task needs a `SerWrite` that writes to USB serial. `ser_write_json::to_writer` calls `write(&[u8])` synchronously. Options:

1. **Buffering SerWrite with block_on flush**: Buffer in `write()`; when buffer reaches ~512 bytes, call `embassy_futures::block_on(Write::write(&tx, &buf))`. Risky in async context but may work if USB write doesn't require other tasks.

2. **Sync USB API**: If esp-hal provides blocking write, use it.

3. **Small buffer + yield points**: Not applicable - we can't yield from within `SerWrite::write`.

**Implementation note**: Start with buffering SerWrite. If block_on causes issues, explore sync USB or alternative approaches.

## Main Components

### 1. OUTGOING_SERVER_MSG
- `static OUTGOING_SERVER_MSG: Channel<CriticalSectionRawMutex, ServerMessage, 1>`
- Replaces OUTGOING_CHUNKS
- Capacity 1 = at most one ServerMessage in flight; backpressure when io_task is slow

### 2. StreamingMessageRouterTransport (Rewritten)
- Holds `channel: &'static Channel<ServerMessage, 1>`
- `async fn send()`: `channel.send(msg).await` (blocks until io_task receives)
- `async fn receive()`: Drain incoming channel, parse ClientMessage (unchanged logic)

### 3. IO Task (Updated)
- Drain OUTGOING_SERVER_MSG first
- On receive: create SerWrite wrapping serial tx, call `to_writer(&msg, &mut writer)`, write M! prefix and newline
- Drain OUTGOING_MSG (log lines) as before
- Read from serial as before

### 4. Async ServerTransport Trait
- `async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>`
- `async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>`
- `async fn close(&mut self) -> Result<(), TransportError>`
- `async fn receive_all` default impl

### 5. Uncommitted Changes to Keep
- lp-model: ser_write_json_tests, SerializableNodeDetail format (serialize_struct_variant, NodeStateSerializer), ser-write-json feature
- justfile: fwtest-json-esp32c6
- fw-esp32 Cargo.toml: ser-write-json dep, test_json feature
