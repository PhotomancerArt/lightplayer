# Async Serial Transport - Design

## Scope of Work

Build an async serial transport for communicating with a remote, async server. The transport will:

1. Run the emulator on a separate thread that loops continuously
2. Communicate with the emulator thread via channels
3. Provide an async `ClientTransport` implementation
4. Include a test similar to `scene_render_emu.rs` but where:
   - The emulator runs on a separate thread looping continuously
   - The test communicates with it async
5. Add lp-cli support for `--push emu` to use emulator mode

## File Structure

```
lp-core/lp-client/src/
├── transport_serial_emu.rs              # EXISTING: Sync version (keep)
├── transport_serial.rs                  # NEW: Generic AsyncSerialClientTransport
├── transport_serial/
│   ├── mod.rs                           # Re-export AsyncSerialClientTransport
│   └── emulator.rs                      # create_emulator_serial_transport_pair()
└── specifier.rs                          # UPDATE: Add HostSpecifier::Emulator variant

lp-core/lp-client/tests/
├── scene_render_emu.rs                  # EXISTING: Sync test
└── scene_render_emu_async.rs            # NEW: Async test with continuous emulator

lp-cli/src/
├── client/
│   └── client_connect.rs                 # UPDATE: Handle HostSpecifier::Emulator
└── main.rs                                # UPDATE: Update help text for --push flag
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Async Test                               │
│  (scene_render_emu_async.rs)                                │
│  or lp-cli dev --push emu                                   │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ uses
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         AsyncSerialClientTransport                          │
│  (Generic - same for emulator and hardware)                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ client_tx: UnboundedSender<ClientMessage>          │   │
│  │ server_rx: UnboundedReceiver<ServerMessage>        │   │
│  │ shutdown_tx: oneshot::Sender<()>                     │   │
│  │ thread_handle: JoinHandle<()>                       │   │
│  └─────────────────────────────────────────────────────┘   │
│  implements ClientTransport                                  │
│  - send(): client_tx.send(msg)                              │
│  - receive(): server_rx.recv().await                         │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ channels (same interface)
                     ▼
         ┌───────────────────────┴───────────────────────┐
         │                                               │
         ▼                                               ▼
┌────────────────────────┐              ┌────────────────────────┐
│  Emulator Thread       │              │  Hardware Serial Thread│
│  (create_emulator_     │              │  (future: create_      │
│   serial_pair)         │              │   hardware_serial_    │
│                        │              │   pair)                │
│  - Owns emulator       │              │  - Owns tokio-serial   │
│  - Reads channels      │              │  - Reads channels      │
│  - Writes to serial    │              │  - Writes to serial   │
│  - Reads from serial   │              │  - Reads from serial   │
│  - Sends to channels   │              │  - Sends to channels  │
└────────────────────────┘              └────────────────────────┘
```

## Main Components

### 1. Generic `AsyncSerialClientTransport`
**Generic transport that doesn't know about emulator vs hardware:**
- Uses channels (`client_tx`, `server_rx`)
- Owns thread handle and shutdown channel for cleanup
- Implements `ClientTransport` trait
- Same interface for emulator and hardware (future)

### 2. `create_emulator_serial_transport_pair()`
**Factory function that knows about emulator:**
- Creates channels (`client_tx/rx`, `server_tx/rx`, `shutdown_tx/rx`)
- Spawns emulator thread
- Returns `AsyncSerialClientTransport` + thread handle
- **Only this function knows about emulator** - transport is generic

### 3. Emulator Thread (internal to factory)
Runs continuously:
- Owns `Riscv32Emulator` instance
- Receives from `client_rx`, writes to serial input
- Calls `run_until_yield()` to step emulator
- Drains serial output, parses messages, sends via `server_tx`
- Checks shutdown signal each iteration

### 4. `HostSpecifier::Emulator`
New variant for lp-cli integration:
- Parses "emu" or "emulator" strings
- `client_connect()` calls `create_emulator_serial_transport_pair()` for this variant

### 5. Future: `create_hardware_serial_transport_pair(port: &str)`
**Future factory function for hardware serial:**
- Same interface as emulator version
- Spawns thread that owns tokio-serial port
- Returns same `AsyncSerialClientTransport` type
- Transport code is reused - no changes needed

## Message Flow

1. **Test/CLI sends message**: `transport.send(msg)` → `client_tx.send(msg)`
2. **Emulator thread receives**: `client_rx.try_recv()` → serialize → `emulator.serial_write()`
3. **Emulator processes**: `run_until_yield()` → firmware processes → sends response via serial
4. **Emulator thread reads**: `emulator.drain_serial_output()` → parse → `server_tx.send(response)`
5. **Test/CLI receives**: `transport.receive()` → `server_rx.recv().await` → returns message

## Key Design Decisions

- **Threading**: `std::thread::spawn` with blocking loop (CPU-intensive, no tokio runtime needed)
- **Channels**: Tokio unbounded channels (`mpsc::UnboundedSender/Receiver`)
- **Time**: `TimeMode::Real` (no manual time advancement)
- **Error Handling**: Channel closure on error (transport detects via `receive()`)
- **Shutdown**: `oneshot::Sender` for graceful shutdown signal
