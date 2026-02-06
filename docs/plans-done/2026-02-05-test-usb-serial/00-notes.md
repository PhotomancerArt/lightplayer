# Plan Notes: Test USB Serial Connection/Disconnection Scenarios

## Scope of Work

The current `fw-esp32` firmware doesn't handle starting without a serial connection well. There's something about how serial is being initialized that doesn't work without a USB host connected. We need to:

1. Set up `test_usb` feature to start an independent task for blinking LEDs (2Hz) on the first LED
2. Create automated tests that verify:
   - LEDs blink correctly
   - Serial works
   - After disconnecting serial, it continues to work
   - Reconnecting works
3. Test under these conditions:
   - Connecting to serial and rebooting
   - Rebooting and connecting to serial
   - Reconnecting to serial
4. Automate these tests using a special feature, placed in `fw-tests`
5. Make the test structure production-ready and compatible with actual `lp-server`, using existing abstractions where possible

## Current State of the Codebase

### Current test_usb Implementation
- Located in `lp-fw/fw-esp32/src/tests/test_usb.rs`
- Currently does LED blinking in the main loop (not a separate task)
- Uses direct async USB serial operations (`usb_rx`/`usb_tx` split)
- Blinks LED at 2Hz (500ms toggle) in the main loop
- Echoes received serial data
- Doesn't count LED blinks
- Doesn't have structured test scenarios

### Serial Implementation
- `Esp32UsbSerialIo` in `lp-fw/fw-esp32/src/serial/usb_serial.rs` uses blocking mode
- Main app uses `Esp32UsbSerialIo` wrapped in `Rc<RefCell<>>` for logging
- Test uses async split (`usb_serial.into_async().split()`)

### LED Output
- `LedChannel` in `lp-fw/fw-esp32/src/output/rmt/channel.rs`
- Uses RMT peripheral for WS2812 LEDs
- `start_transmission()` returns `LedTransaction` that must be waited on
- Currently used synchronously in test_usb

### Embassy Executor
- Main entry point uses `#[esp_rtos::main]` with `Spawner` parameter
- Can spawn tasks using `spawner.spawn(task())`
- Current test_usb doesn't use spawner (LED blinking is in main loop)

### fw-tests Structure
- Located in `lp-fw/fw-tests/`
- Currently has `transport_emu_serial.rs` for emulator transport
- Has example test `scene_render_emu.rs` showing how to build firmware, load ELF, create transport
- Uses tokio for async runtime on host side

### Build/Flash Tools
- `just demo-esp32` command exists (uses `cargo-espflash`)
- Uses `cargo run --target riscv32imac-unknown-none-elf --release --features esp32c6`
- `cargo-espflash` handles flashing and monitoring

## Notes

### Main Loop Structure (from user feedback)
The test should simulate the real firmware structure:
- Main loop pattern: `blink_led()` then `handle_messages()`
- Message queue system (strings) that can be adapted for real code
- Handle message limiting (what happens if we get too many messages?)
- Design abstractions in `fw-core` so they can be tested independently of hardware
- As much as possible abstracted from real hardware, but still need real hardware testing

### Architecture Direction
- Put main abstractions in `fw-core` for testability
- Message queue should be reusable for real code
- Test structure should mirror production structure

## Questions That Need to be Answered

1. **Task Structure**: ✅ ANSWERED - Use main loop pattern: `blink_led()` then `handle_messages()`. Spawn LED task immediately, but structure should simulate real firmware main loop.

2. **Message Queue Design**: ✅ ANSWERED - Make it generic `MessageQueue<T>` so it can handle String, ClientMessage, ServerMessage, etc. 

### Outgoing Queue Considerations (from user feedback)
- **Without queue**: Need direct access to serial handle, but if write blocks/fails, main loop could stall
- **With queue**: Decouples main loop from I/O failures, but adds complexity
- **Multi-transport future**: Will have serial + websockets + bluetooth clients
- **Architecture implication**: Need to design for multiple transports, not just one

### Architecture Direction
- Outgoing queue makes sense to prevent main loop blocking on I/O failures
- Design should support multiple transports (each with own queues?)
- Main loop should be transport-agnostic
- Separate I/O tasks for each transport that drain queues

### Message Queue Architecture Analysis

**Current Pattern:**
- Server loop collects incoming messages into `Vec<ClientMessage>` by calling `transport.receive()` in a loop
- Outgoing messages are sent immediately via `transport.send(ServerMessage)` - no queue

**Proposed Design:**
- `MessageQueue<T>` generic queue in `fw-core`
- Incoming: `MessageQueue<String>` for test, `MessageQueue<ClientMessage>` for real code
- Outgoing: Probably not needed initially (send immediately), but generic design allows adding `MessageQueue<ServerMessage>` later if needed

**Benefits:**
- Bounded queue with overflow handling (drop oldest on overflow)
- Testable independently of hardware
- Reusable for real code
- Generic design allows future extensions

3. **Multi-Transport Architecture**: ✅ ANSWERED - Design for multi-transport future, but implement single transport for now. Use queues to decouple main loop from I/O. Structure should be extensible.

### Architecture Decisions (Single Transport, Multi-Transport Ready)
- **MessageQueue<T>** in `fw-core` - generic, reusable
- **Incoming queue**: `MessageQueue<String>` for test, `MessageQueue<ClientMessage>` for real
- **Outgoing queue**: `MessageQueue<String>` for test, `MessageQueue<ServerMessage>` for real
- **I/O task**: Separate task that drains queues and handles hardware I/O
- **Main loop**: Transport-agnostic, works with queues
- **Future**: Can extend to TransportManager pattern with multiple transports

4. **Serial Initialization**: ✅ ANSWERED - Try to initialize serial, but don't block on it. Use state management (Uninitialized, Ready, Disconnected, Error). I/O task handles state transitions and can retry initialization.

5. **Message Queue Implementation**: ✅ ANSWERED - Use `embassy-sync::channel::Channel` (see `00-research.md` for details)
   - Already in dependency tree (via embassy-executor)
   - no_std, no-alloc, async-compatible
   - MPMC (multi-transport ready)
   - Bounded (configurable capacity)
   - Cross-platform (ESP32, RISC-V, etc.)
   - Production-ready

### Message Router Architecture
- Use `embassy-sync::channel::Channel` for queues
- `MessageRouter` in `fw-core` that manages incoming/outgoing channels
- Main loop calls `router.receive_all()` and `router.send()`
- I/O task drains queues and handles hardware
- Overflow: `try_send()` returns error if full, can implement "drop oldest" strategy

6. **Frame Counter**: ✅ ANSWERED - Use frame counter (not blink count) to verify main loop is running continuously.
   - Frame counter incremented each main loop iteration
   - Tight loop so counter always increases between requests
   - Use atomic counter (`AtomicU32`) owned by main loop
   - Query via `M!{"get_frame_count":{}}\n` -> `M!{"frame_count":12345}\n`
   - LED blinking is separate - just for human visual observation (2Hz blink)

7. **Message Protocol**: ✅ ANSWERED - Use JSON with prefix "M!" to reject non-message data. Format: `M!{...}\n`

### Test Message Protocol
- **Prefix**: `M!` - filters out non-message data (debug prints, etc.)
- **Format**: `M!{...}\n` (JSON terminated with newline)
- **External discriminators**: Use serde-json-core external discriminators for efficiency
- **Command naming**: Commands start with verbs
- **Commands**:
  - `M!{"get_frame_count":{}}\n` - Query frame counter (verifies main loop running)
  - `M!{"echo":{"data":"test"}}\n` - Echo test message
- **Responses**:
  - `M!{"frame_count":12345}\n` - Frame count response (always increases)
  - `M!{"echo":"test"}\n` - Echo response
- **Note**: This prefix pattern should also be used in main system (`SerialTransport`) to filter out debug prints

### Current SerialTransport
- Currently expects plain JSON: `{...}\n`
- Should be updated to expect `M!{...}\n` prefix
- This will allow filtering out non-message serial data

3. **LED Blink Counting**: ✅ ANSWERED - LED blinking is separate from frame counter, just for human visual observation (2Hz blink). Frame counter is used for automated verification.

8. **Test Automation**: ✅ ANSWERED - Host-side test automation in `fw-tests`:
   - Build firmware with `test_usb` feature
   - Flash firmware to ESP32 using `cargo-espflash`
   - Monitor serial output (parse `M!{...}\n` messages)
   - Send commands via serial (`M!{"get_frame_count":{}}\n`, `M!{"echo":{"data":"test"}}\n`)
   - Disconnect/reconnect serial (using serial port manipulation)
   - Reset ESP32 (using `cargo-espflash reset` or serial DTR/RTS)
   - Verify frame counts before/after disconnection (proves main loop continued)

### Test Scenarios
- **Scenario 1**: Flash → Wait → Connect serial → Query frame count → Verify LEDs blink (visual) → Disconnect → Wait → Reconnect → Query frame count → Verify count increased (proves main loop continued)
- **Scenario 2**: Flash → Connect serial immediately → Query frame count → Verify serial works → Disconnect → Wait → Reconnect → Query frame count → Verify count increased
- **Scenario 3**: Flash → Connect serial → Send echo → Verify echo → Disconnect → Reconnect → Send echo → Verify echo → Query frame count → Verify count increased

### Tools Needed
- Serial port library (e.g., `serialport` crate)
- `cargo-espflash` integration for flashing/resetting
- Message parser for `M!{...}\n` format
- Test framework (tokio test)

9. **Serial Connection Detection**: ✅ ANSWERED - Poll-based detection (simpler, more reliable):
   - I/O task periodically checks serial state
   - Try to read/write, handle errors gracefully
   - Track state: `Uninitialized`, `Ready`, `Disconnected`, `Error`
   - On error, mark as `Disconnected` and retry periodically
   - For ESP32 USB Serial: `UsbSerialJtag::new()` may succeed even without host, but read/write will fail if not connected

10. **Reset Mechanism**: ✅ ANSWERED - Use `cargo-espflash reset` command or serial port DTR/RTS control for host-side reset. No need for serial reset command (would require serial to be working).

7. **Test Protocol**: ✅ ANSWERED - See Message Protocol section above (JSON with `M!` prefix)

11. **Production-Ready Structure**: ✅ ANSWERED - See architecture decisions above. Create abstractions in `fw-core` (MessageRouter, message queues) that can be reused by main app. LED blinking can be abstracted if needed, but for now keep it simple.

12. **Feature Flag**: ✅ ANSWERED - Replace `test_usb` entirely (it's currently broken). Automated tests go in `fw-tests` crate. Firmware code stays in `fw-esp32` with `test_usb` feature.

13. **Timing**: ✅ ANSWERED - Test scenarios should run long enough to verify continuous operation:
   - Wait periods: 1-2 seconds (enough to verify main loop continues)
   - Frame counter queries: Should show increase between queries (proves tight loop)
   - Total test time: ~10-30 seconds per scenario (reasonable for automated tests)
