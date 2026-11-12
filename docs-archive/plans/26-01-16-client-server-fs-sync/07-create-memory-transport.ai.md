# Phase 7: Create In-Memory Transport for Tests

## Goal

Create in-memory transport implementation that serializes/deserializes messages to/from JSON for testing.

## Tasks

1. Create `lp-client/src/transport/mod.rs`:
   - Export `memory` module

2. Create `lp-client/src/transport/memory.rs`:
   - Define `MemoryTransport` struct:
     ```rust
     pub struct MemoryTransport {
         client_tx: Sender<Vec<u8>>,  // Raw bytes (JSON)
         server_rx: Receiver<Vec<u8>>,
         server_tx: Sender<Vec<u8>>,
         client_rx: Receiver<Vec<u8>>,
     }
     ```
   - Or use `mpsc::channel()` for bidirectional communication
   - Implement `ClientTransport`:
     - `send()`: Serialize `Message` to JSON, send bytes
     - `receive()`: Receive bytes, deserialize JSON to `Message`
   - Implement `ServerTransport` (same interface)
   - Create `new()` function that returns `(MemoryTransport, MemoryTransport)` pair (client and server sides)

3. Alternative: Use a shared queue/channel:
   - Single channel with message direction indicator
   - Or two separate channels (client->server and server->client)

4. Ensure JSON serialization:
   - Use `serde_json::to_vec()` and `serde_json::from_slice()`
   - Handle serialization errors appropriately
   - This ensures the message protocol serializes correctly

5. Add to `lp-client/src/lib.rs`:
   - Export `transport` module (or keep internal for now)

6. Consider making it available in `lp-shared`:
   - Or keep in `lp-client` since it's primarily for testing client
   - Could create separate test utilities crate

## Success Criteria

- `MemoryTransport` exists and implements `ClientTransport` and `ServerTransport`
- Messages are serialized to/from JSON
- Can create client/server transport pair for testing
- All code compiles without warnings
