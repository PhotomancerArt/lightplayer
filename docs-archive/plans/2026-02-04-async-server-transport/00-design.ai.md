# Design: Async ServerTransport

## Scope of Work

Make `ServerTransport` async to solve ESP32 USB serial deadlock issues and improve performance for async-first platforms. This is a breaking change that affects all transport implementations and server loops.

## File Structure

```
lp-core/lp-shared/src/transport/
├── server.rs              # UPDATE: ServerTransport trait (async methods)
└── mod.rs                 # UPDATE: Export async ServerTransport

lp-fw/fw-core/src/
├── transport/
│   └── serial.rs          # UPDATE: SerialTransport (async, direct async I/O)
└── serial/
    └── io.rs              # REMOVE: SerialIo trait no longer needed

lp-fw/fw-esp32/src/
├── serial/
│   └── usb_serial.rs      # UPDATE: Direct async USB serial (no SerialIo wrapper)
└── server_loop.rs          # UPDATE: Use async transport (.await)

lp-fw/fw-emu/src/
├── serial.rs               # UPDATE: Async adapter wrapping sync syscalls
└── server_loop.rs          # UPDATE: Use block_on to call async transport

lp-cli/src/server/
├── transport_ws.rs         # UPDATE: Make async (already uses async internally)
└── run_server_loop_async.rs # UPDATE: Use async transport (.await)

lp-cli/src/commands/serve/
└── server_loop.rs          # UPDATE: Use block_on to call async transport

lp-core/lp-client/src/
└── local.rs                # UPDATE: Make AsyncLocalServerTransport async

lp-core/lp-server/src/
└── server.rs                # NO CHANGE: Stays sync, doesn't call transport
```

## Conceptual Architecture

### Current Architecture (Sync)
```
Server Loop (sync/async)
    ↓
LpServer::tick() (sync)
    ↓
ServerTransport (sync trait)
    ├─→ SerialTransport
    │   └─→ SerialIo (sync trait)
    │       ├─→ Esp32UsbSerialIo (uses block_on ❌ deadlock)
    │       └─→ SyscallSerialIo (sync syscalls)
    ├─→ WebSocketServerTransport (sync wrapper)
    └─→ AsyncLocalServerTransport (sync wrapper)
```

### New Architecture (Async)
```
Server Loop (async)
    ↓
LpServer::tick() (sync) - no change
    ↓
ServerTransport (async trait)
    ├─→ SerialTransport (async)
    │   ├─→ ESP32: Direct async USB serial ✅
    │   └─→ fw-emu: Async adapter for sync syscalls ✅
    ├─→ WebSocketServerTransport (async) ✅
    └─→ AsyncLocalServerTransport (async) ✅

Server Loop (sync)
    ↓
LpServer::tick() (sync) - no change
    ↓
block_on() wrapper
    ↓
ServerTransport (async trait) ✅
```

## Main Components

### 1. ServerTransport Trait (Async)
- `send()` → `async fn send()`
- `receive()` → `async fn receive()`
- `close()` → `async fn close()`
- `receive_all()` → `async fn receive_all()` (default implementation)

### 2. SerialTransport (Async)
- Removes dependency on `SerialIo` trait
- ESP32: Uses async USB serial directly (`embedded_io_async::Write/Read`)
- fw-emu: Wraps sync syscalls in async adapter

### 3. Server Loops
- **ESP32**: Already async → use `.await` directly
- **fw-emu**: Sync → use `block_on()` to call async transport (safe in sync context)
- **CLI async**: Already async → use `.await` directly
- **CLI sync**: Sync → use `block_on()` to call async transport

### 4. LpServer
- **No changes needed** - doesn't call transport directly
- Server loops handle transport I/O before/after `tick()`

## Key Design Decisions

1. **Single async trait** - No separate sync/async versions
2. **LpServer stays sync** - Pure business logic, no I/O
3. **Server loops handle async** - They already manage transport I/O
4. **fw-emu uses block_on** - Safe in sync context, preserves simple architecture
5. **Remove SerialIo** - Not needed if transport is async
6. **All-at-once migration** - Cleaner, avoids temporary compatibility code

## Benefits

1. **Solves ESP32 deadlock** - No more `block_on` in async context
2. **Better performance** - Direct async operations for async platforms
3. **Simpler architecture** - One less abstraction layer (SerialIo)
4. **Future-proof** - Aligned with async-first platforms
5. **Maintainable** - Single trait to maintain

## Migration Impact

- **Breaking change** - All `ServerTransport` implementations must be updated
- **Server loops** - Must handle async transport calls
- **fw-emu** - Needs `block_on` wrapper (but safe in sync context)
- **Tests** - Must be updated to use async transport
