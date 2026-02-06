# Design: Apply test_usb Structure to Main fw-esp32 App

## Scope of Work

Apply the architecture and patterns from `test_usb` to the main `fw-esp32` application so that it:
1. Starts correctly without a serial connection
2. Handles serial connection/disconnection gracefully
3. Uses `MessageRouter` to decouple I/O from the main loop
4. Implements proper serial transport using the JSON protocol with `M!` prefix
5. Continues operating when serial is disconnected
6. Updates `SerialTransport` to also support `M!` prefix for consistency

## File Structure

```
lp-fw/
├── fw-core/
│   └── src/
│       └── transport/
│           ├── mod.rs                    # UPDATE: Export message_router
│           ├── serial.rs                 # UPDATE: Add M! prefix support
│           └── message_router.rs         # NEW: MessageRouterTransport wrapper
│
└── fw-esp32/
    └── src/
        ├── main.rs                       # UPDATE: Use MessageRouterTransport, spawn I/O task
        ├── server_loop.rs                # UPDATE: Use MessageRouterTransport instead of FakeTransport
        └── serial/
            ├── mod.rs                    # UPDATE: Export io_task
            └── io_task.rs                # NEW: Async I/O task for serial communication
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Main Loop                              │
│  (server_loop.rs)                                          │
│                                                             │
│  ┌─────────────────────────────────────┐                   │
│  │  MessageRouterTransport             │                   │
│  │  (implements ServerTransport)        │                   │
│  │  - receive() → reads from router     │                   │
│  │  - send() → writes to router         │                   │
│  └──────────────┬──────────────┬────────┘                   │
│                 │              │                             │
│                 ▼              ▼                             │
│         ┌──────────────┐ ┌──────────────┐                   │
│         │  INCOMING    │ │  OUTGOING    │                   │
│         │  Channel     │ │  Channel     │                   │
│         │  (String)    │ │  (String)    │                   │
│         └──────┬───────┘ └───────┬──────┘                   │
└────────────────┼─────────────────┼─────────────────────────┘
                 │                 │
                 │                 │
┌────────────────┼─────────────────┼─────────────────────────┐
│                │                 │                           │
│         ┌──────▼─────────────────▼──────┐                    │
│         │      I/O Task (io_task.rs)    │                    │
│         │                                │                    │
│         │  ┌────────────────────────┐   │                    │
│         │  │  Read from Serial      │   │                    │
│         │  │  - Filter M! prefix    │   │                    │
│         │  │  - Push to INCOMING    │   │                    │
│         │  └───────────┬────────────┘   │                    │
│         │              │                 │                    │
│         │  ┌───────────▼────────────┐   │                    │
│         │  │  Drain OUTGOING        │   │                    │
│         │  │  - Add M! prefix       │   │                    │
│         │  │  - Write to Serial     │   │                    │
│         │  └───────────────────────┘   │                    │
│         └──────────────┬────────────────┘                    │
│                        │                                      │
│                        ▼                                      │
│              ┌─────────────────┐                             │
│              │  USB Serial     │                             │
│              │  (tx/rx split)  │                             │
│              └─────────────────┘                             │
│                        │                                      │
│                        ▼                                      │
│              ┌─────────────────┐                             │
│              │  Host Client     │                             │
│              └─────────────────┘                             │
└──────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. MessageRouterTransport (`fw-core/src/transport/message_router.rs`)
- Wraps `MessageRouter` and implements `ServerTransport` trait
- Converts between `String` (router messages) and `ClientMessage`/`ServerMessage` (transport interface)
- Uses `try_receive()` for non-blocking reads from router
- Serializes/deserializes JSON messages with `M!` prefix handling

### 2. I/O Task (`fw-esp32/src/serial/io_task.rs`)
- Async task spawned at startup
- Handles serial I/O independently of main loop
- Reads from serial, filters `M!` prefix, pushes to `INCOMING` channel
- Drains `OUTGOING` channel, adds `M!` prefix, writes to serial
- Handles serial initialization/retry gracefully (doesn't block main loop)
- Manages serial connection state (Ready/Disconnected/Error)

### 3. Main Loop Updates (`fw-esp32/src/main.rs`, `server_loop.rs`)
- Initialize serial early for logging (handle failures gracefully)
- Create `MessageRouter` with static channels
- Spawn I/O task with USB device peripheral
- Create `MessageRouterTransport` and pass to server loop
- Server loop uses `MessageRouterTransport` instead of `FakeTransport`
- Main loop continues even if serial initialization fails

### 4. SerialTransport Update (`fw-core/src/transport/serial.rs`)
- Add `M!` prefix support for sending messages
- Filter non-message lines (without `M!` prefix) when receiving
- Maintain backward compatibility for `fw-emu` (can handle both formats during transition)

## Key Design Decisions

1. **MessageRouterTransport**: Wrapper pattern keeps concerns separated - router handles task communication, transport handles protocol conversion
2. **M! Prefix**: Used consistently across all transports to filter debug output and distinguish messages
3. **Graceful Startup**: Serial initialization failures don't prevent main loop from starting
4. **Async I/O**: I/O task handles serial communication asynchronously, allowing main loop to run independently
5. **Shared Serial**: Same USB serial instance used for both logging and transport, distinguished by `M!` prefix
