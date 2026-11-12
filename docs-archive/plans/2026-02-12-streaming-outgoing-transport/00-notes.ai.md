# Plan: Streaming Outgoing Transport

## Scope of Work

Redesign outgoing message handling for ESP32 firmware to avoid large buffer allocations. Outgoing messages (e.g. GetChanges with fixture/output channel data) can exceed 20KB when serialized to JSON. With ~200KB free heap and the compiler as a memory consumer, we cannot buffer full messages. Implement direct streaming: serialize in the IO task and write directly to serial, only buffering small amounts at a time.

**Secondary scope:** Clean up uncommitted changes. Some are useful for the new design; some (the chunked channel approach) will be replaced.

## Current State

### Message Flow
- **Server loop** (main task): Calls `transport.send(ServerMessage)`, receives `ClientMessage` via `transport.receive()`
- **IO task**: Owns USB serial; drains outgoing channel, writes to serial; reads from serial, pushes to incoming channel
- **Transport**: `StreamingMessageRouterTransport` (staged) serializes via `ChunkingSerWrite` → 512-byte chunks → `Channel<Vec<u8>, 256>` → io_task drains and writes

### Problem with Current Chunked Approach
- `Channel<Vec<u8>, 256>` with 512-byte chunks = up to 128KB queued
- Serialization completes quickly; chunks pile up before serial drains
- Peak memory dominated by channel, not avoided

### lp-model Changes (Uncommitted)
- **json.rs**: Adds `ser_write_json_tests` module - validates ser-write-json produces JSON compatible with serde-json-core deserializer. **Useful** - keep.
- **project/api.rs**: Changes `SerializableNodeDetailWithFrame` to use `serialize_struct_variant` and `NodeStateSerializer` - required for ser-write-json to produce correct externally-tagged enum format. **Useful** - keep.
- **Cargo.toml**: Adds `ser-write-json` feature and dependency. **Useful** - keep.

### fw-esp32 Changes (Uncommitted + Staged)
- **transport.rs** (staged): `StreamingMessageRouterTransport` with `ChunkingSerWrite`. **Replace** - chunk channel approach doesn't scale.
- **io_task.rs**: Adds `OUTGOING_CHUNKS`, `get_chunk_channel()`. **Replace** - use `OUTGOING_SERVER_MSG` instead.
- **main.rs**: Uses `StreamingMessageRouterTransport`, adds `test_json` feature gate. **Update** - use new transport.
- **test_json.rs** (staged): Validates ser-write-json on device; uses MessageRouter with String (not StreamingMessageRouterTransport). **Adapt or remove** - test validates ser-write-json boots and works; may simplify to match new architecture.
- **Cargo.toml**: ser-write-json dep, test_json feature. **Keep** dep, **evaluate** test_json.
- **justfile**: `fwtest-json-esp32c6` recipe. **Keep** if test_json remains.

### Other Transports (Desktop, WebSocket)
- Buffer full messages in memory; no change needed for this plan.
- `ServerTransport` trait stays the same unless we need async `send` for blocking backpressure.

## Design Decisions (from prior discussion)

1. **Focus on JSON** - Avoid full JSON buffer; in-memory ServerMessage size is secondary for now.
2. **Compiler runs in update cycle** - Incoming messages are consumed and dropped; some trigger compiler; then we generate outgoing message. No need to hold incoming large data.
3. **Blocking on serial OK** - `transport.send()` can block until IO task drains; provides backpressure.
4. **Serialize in IO task** - Pass `ServerMessage` through `Channel<ServerMessage, 1>`. IO task receives, serializes with `ser-write-json` directly to serial writer. Never buffer full JSON.

## Questions

### Q1: Async send for blocking backpressure?

**Context:** With `Channel<ServerMessage, 1>`, when the channel is full (IO task hasn't received), `transport.send()` must wait. Embassy `Channel::send()` is async. Current `ServerTransport::send` is sync.

**Options:**
- **A:** Make `ServerTransport::send` async - breaking change, all transports must implement.
- **B:** Poll loop with yield - `try_send` in loop, `Timer::after(1ms).await` when full. Transport stays sync but needs access to async runtime. Awkward.
- **C:** Return error when full, let caller retry - pushes complexity to server loop.

**Suggested:** Option A - make send async. Plan 2026-02-04-async-server-transport already designed this. We can do it as part of this work or note as dependency.

**Decision:** Option A - make ServerTransport::send async.

### Q2: What to do with test_json?

**Context:** test_json validates ser-write-json on ESP32 hardware (boot, serialization). It uses a minimal io_task and MessageRouter with String (not the new ServerMessage channel). The new architecture uses ServerMessage channel and serialization in io_task.

**Options:**
- **A:** Remove test_json - main app with demo_project provides sufficient validation.
- **B:** Keep test_json, adapt to use new transport (Channel<ServerMessage, 1>, io_task serializes) - validates ser-write-json path on device without full server.
- **C:** Simplify test_json to only boot and send one Heartbeat via new transport - minimal smoke test.

**Suggested:** Option B or C - retain a hardware validation path for ser-write-json.

**Decision:** Option B - Keep test_json, adapt to use new transport (Channel<ServerMessage, 1>, io_task serializes).

### Q3: Relationship to 2026-02-04-async-server-transport plan?

**Context:** That plan makes ServerTransport fully async (send, receive, close). Our work needs async send for blocking backpressure. Receive can stay sync (returns Option, non-blocking).

**Options:**
- **A:** Implement full async ServerTransport as in that plan - larger scope.
- **B:** Add only async send, keep receive sync - minimal change for our needs.
- **C:** Do our work with sync send + poll loop (option B from Q1) - avoid trait change.

**Suggested:** Option B - async send only, if we determine that's sufficient. Or Option A if we want to align with the async plan.

**Decision:** Option A - Implement full async ServerTransport as in 2026-02-04-async-server-transport plan.

## Notes

- Chunked API (ChunkingSerWrite, OUTGOING_CHUNKS) is not useful - channel buffers too much.
- ser-write-json SerWrite can wrap anything that implements write; we need a SerWrite that writes to embedded_io_async::Write.
- Log lines stay on OUTGOING_MSG (String, small); server messages use new OUTGOING_SERVER_MSG.
