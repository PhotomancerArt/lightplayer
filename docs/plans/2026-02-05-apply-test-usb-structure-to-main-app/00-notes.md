# Notes: Apply test_usb Structure to Main fw-esp32 App

## Scope of Work

Apply the architecture and patterns from `test_usb` to the main `fw-esp32` application so that it:
1. Starts correctly without a serial connection
2. Handles serial connection/disconnection gracefully
3. Uses `MessageRouter` to decouple I/O from the main loop
4. Implements proper serial transport using the JSON protocol with `M!` prefix
5. Continues operating when serial is disconnected

## Current State

### Main App (`fw-esp32/src/main.rs`)
- Currently uses `FakeTransport` (no-op transport for testing)
- Initializes USB serial **synchronously** for logging only
- USB serial initialization happens **before** runtime starts
- Uses `esp_println` for logging, which requires USB serial to be initialized
- Server loop runs with `FakeTransport` - no real serial communication
- **Problem**: Doesn't start without serial connection (USB serial init blocks/fails)

### test_usb Architecture (What We Want to Apply)
- Uses `MessageRouter` with embassy-sync channels for task communication
- Separate I/O task that handles serial communication asynchronously
- Main loop handles business logic (LED blinking, message processing)
- Heartbeat task sends periodic status messages
- Serial initialization happens in I/O task, doesn't block main loop
- Messages use `M!` prefix to filter out non-message data
- Handles disconnection/reconnection gracefully

### Current Serial Transport (`fw-core/src/transport/serial.rs`)
- Implements `ServerTransport` trait
- Uses `SerialIo` trait for raw byte I/O
- Handles JSON framing (`\n` termination)
- **Does NOT** use `M!` prefix (expects plain JSON)
- **Does NOT** filter non-message data
- Synchronous/blocking I/O operations

### Current Server Loop (`fw-esp32/src/server_loop.rs`)
- Calls `transport.receive()` in a loop (blocking)
- Calls `transport.send()` for responses
- Runs at ~60 FPS with 1ms delay between frames
- Currently uses `FakeTransport`

## Questions

1. **Serial Transport Integration**: ✅ ANSWERED - We should **replace `SerialTransport`** for ESP32 with a `MessageRouter`-based approach. `SerialTransport` can remain for `fw-emu` (it works fine there with sync I/O), but for ESP32 we'll use `MessageRouter` with async I/O.
   - **Decision**: Create `MessageRouterTransport` wrapper that implements `ServerTransport` and uses `MessageRouter` internally
   - `MessageRouter` continues to use `String` messages (JSON strings with `M!` prefix)
   - `MessageRouterTransport` converts between `String` (router) and `ClientMessage`/`ServerMessage` (transport)
   - I/O task handles serial I/O and message conversion
   - Server loop uses `MessageRouterTransport` synchronously (via `try_receive()` from router)

2. **Message Protocol**: ✅ ANSWERED - Use `M!` prefix for the new `MessageRouterTransport`. This is consistent with `test_usb` and helps filter debug output. We should also update `SerialTransport` to support `M!` prefix (can be done in a separate phase or as part of this work).

3. **Logging vs Transport**: ✅ ANSWERED - Both logging and transport use the same USB serial interface. The `M!` prefix distinguishes messages from log output.
   - Try to initialize serial early for logging, but handle failures gracefully
   - Main loop should start even if serial initialization fails
   - I/O task can retry serial initialization if it fails initially
   - Share the same USB serial instance (split into tx/rx halves)

4. **Startup Behavior**: ✅ ANSWERED - Try to initialize serial early for logging, but start the main loop even if it fails. The I/O task will handle serial initialization/retry asynchronously (like `test_usb`).

5. **Heartbeat Messages**: ✅ ANSWERED - Heartbeat messages should be sent by `lp-server` as proper `ServerMessage` types (with `M!` prefix). This makes them part of the protocol rather than a separate debugging mechanism. Clients can subscribe or ignore them.

6. **Backward Compatibility**: ✅ ANSWERED - Update `SerialTransport` to also use `M!` prefix so clients can talk to both transports consistently. This should be part of this plan.
