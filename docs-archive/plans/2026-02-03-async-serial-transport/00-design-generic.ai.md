# Async Serial Transport - Generic Design

## Key Insight

The transport itself should be **generic** - it only uses channels and doesn't know about emulator vs hardware. The **pair creation function** is what knows the implementation details.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              AsyncSerialClientTransport                      │
│  (Generic - just uses channels, same as AsyncLocalClient)    │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ client_tx: UnboundedSender<ClientMessage>          │   │
│  │ server_rx: UnboundedReceiver<ServerMessage>          │   │
│  │ shutdown_tx: oneshot::Sender<()>                     │   │
│  │ thread_handle: JoinHandle<()>                        │   │
│  └─────────────────────────────────────────────────────┘   │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ channels (same interface)
                     ▼
         ┌───────────────────────┴───────────────────────┐
         │                                               │
         ▼                                               ▼
┌────────────────────────┐              ┌────────────────────────┐
│  Emulator Thread       │              │  Hardware Serial Thread│
│  (create_emulator_      │              │  (create_hardware_     │
│   serial_pair)         │              │   serial_pair)         │
│                        │              │                        │
│  - Owns emulator       │              │  - Owns tokio-serial   │
│  - Reads channels      │              │  - Reads channels      │
│  - Writes to serial    │              │  - Writes to serial    │
│  - Reads from serial   │              │  - Reads from serial   │
│  - Sends to channels   │              │  - Sends to channels   │
└────────────────────────┘              └────────────────────────┘
```

## Design

### 1. Generic `AsyncSerialClientTransport`

Same interface as `AsyncLocalClientTransport` - just uses channels:
- Takes `client_tx`, `server_rx`, `shutdown_tx`, `thread_handle` in constructor
- Implements `ClientTransport` trait
- Doesn't know or care about emulator vs hardware

### 2. `create_emulator_serial_transport_pair()`

Creates channels + spawns emulator thread:
- Creates channels (`client_tx/rx`, `server_tx/rx`, `shutdown_tx/rx`)
- Spawns thread that:
  - Owns `Riscv32Emulator`
  - Loops: check shutdown → process `client_rx` → write to emulator serial → `run_until_yield()` → drain serial → parse → send via `server_tx`
- Returns `AsyncSerialClientTransport` + thread handle

### 3. `create_hardware_serial_transport_pair(port: &str)` (future)

Creates channels + spawns hardware serial thread:
- Creates channels (same as emulator)
- Spawns thread that:
  - Owns tokio-serial port
  - Loops: check shutdown → process `client_rx` → write to serial port → read from serial port → parse → send via `server_tx`
- Returns `AsyncSerialClientTransport` + thread handle

## Benefits

1. **Transport is generic** - can be reused for emulator and hardware
2. **Pair creation is specific** - only the factory function knows implementation details
3. **Easy to add hardware support** - just add `create_hardware_serial_transport_pair()`
4. **Same interface** - both use same `AsyncSerialClientTransport` type

## File Structure

```
lp-core/lp-client/src/
├── transport_serial.rs                    # NEW: Generic AsyncSerialClientTransport
├── transport_serial_emu.rs                # EXISTING: Sync version (keep for now)
└── specifier.rs                            # UPDATE: Add HostSpecifier::Emulator

lp-core/lp-client/src/transport_serial/
├── mod.rs                                  # Re-export AsyncSerialClientTransport
├── emulator.rs                             # create_emulator_serial_transport_pair()
└── hardware.rs                             # FUTURE: create_hardware_serial_transport_pair()
```

## Implementation Details

### `AsyncSerialClientTransport`

```rust
pub struct AsyncSerialClientTransport {
    client_tx: Option<mpsc::UnboundedSender<ClientMessage>>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread_handle: Option<JoinHandle<()>>,
    closed: bool,
}

impl AsyncSerialClientTransport {
    /// Create from channels (internal - use factory functions)
    pub(crate) fn new(
        client_tx: mpsc::UnboundedSender<ClientMessage>,
        server_rx: mpsc::UnboundedReceiver<ServerMessage>,
        shutdown_tx: oneshot::Sender<()>,
        thread_handle: JoinHandle<()>,
    ) -> Self {
        // ...
    }
}
```

### `create_emulator_serial_transport_pair()`

```rust
pub fn create_emulator_serial_transport_pair(
    emulator: Arc<Mutex<Riscv32Emulator>>,
) -> Result<AsyncSerialClientTransport> {
    // Create channels
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    
    // Spawn emulator thread
    let thread_handle = std::thread::spawn(move || {
        emulator_thread_loop(emulator, client_rx, server_tx, shutdown_rx);
    });
    
    Ok(AsyncSerialClientTransport::new(client_tx, server_rx, shutdown_tx, thread_handle))
}
```

## Comparison with Existing Patterns

- **Similar to `AsyncLocalClientTransport`**: Both use channels, transport doesn't know implementation
- **Different from `LocalServerTransport`**: That wraps `AsyncLocalClientTransport`, we're creating the transport directly
- **Future hardware**: Same transport type, different factory function
