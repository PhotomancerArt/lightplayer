# Phase 10: Implement ESP32 serial communication protocol handler

## Goal

Implement the transport trait for ESP32 using serial port.

## Tasks

1. Add serial/UART dependencies to `fw-esp32/Cargo.toml`
2. Create `src/transport.rs`:
   - Implement `lp-core::traits::Transport` trait for ESP32
   - Use esp-hal UART for serial communication
   - Implement `send_message()` - write JSON string + `\n`
   - Implement `receive_message()` - read until `\n`, return string
   - Handle partial messages (buffer until `\n`)
   - Parse errors should warn and ignore (as specified)
3. Initialize UART in `main.rs`:
   - Set up UART at 115200 baud
   - Create transport instance
4. Test basic message send/receive

## Success Criteria

- Transport trait implementation works
- Can send/receive JSON messages over serial
- Partial messages are handled correctly
- Parse errors are ignored with warnings
- All code compiles without warnings

