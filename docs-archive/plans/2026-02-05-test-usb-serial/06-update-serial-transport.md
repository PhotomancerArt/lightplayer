# Phase 6: Update SerialTransport to Support M! Prefix

## Scope of phase

Update `SerialTransport` in `fw-core` to support the `M!` prefix pattern. This allows filtering out non-message data (debug prints) and makes the transport more robust.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update SerialTransport receive method

Update `lp-fw/fw-core/src/transport/serial.rs`:

```rust
impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    // ... existing send method ...

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Read available bytes in a loop until we have a complete message or no more data
        let mut temp_buf = [0u8; 256];
        loop {
            match self.io.read_available(&mut temp_buf) {
                Ok(n) => {
                    if n > 0 {
                        log::trace!("SerialTransport: Read {n} bytes from serial");
                        // Append to read buffer
                        self.read_buffer.extend_from_slice(&temp_buf[..n]);
                        log::trace!(
                            "SerialTransport: Read buffer now has {} bytes",
                            self.read_buffer.len()
                        );
                    } else {
                        // No data available - break and check for complete message
                        log::trace!("SerialTransport: read_available returned 0, no more data");
                        break;
                    }
                }
                Err(e) => {
                    log::warn!("SerialTransport: Serial read error: {e}");
                    return Err(TransportError::Other(format!("Serial read error: {e}")));
                }
            }

            // Check if we have a complete message after reading
            if self.read_buffer.iter().any(|&b| b == b'\n') {
                break;
            }
        }

        // Look for complete message (ends with \n)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            log::trace!(
                "SerialTransport: Received complete message ({} bytes)",
                newline_pos + 1
            );

            // Extract message (without \n)
            let message_bytes: Vec<u8> = self.read_buffer.drain(..=newline_pos).collect();
            let message_str = match str::from_utf8(&message_bytes[..message_bytes.len() - 1]) {
                Ok(s) => s,
                Err(_) => {
                    // Invalid UTF-8, ignore with warning
                    #[cfg(any(feature = "emu", feature = "esp32"))]
                    log::warn!("SerialTransport: Invalid UTF-8 in message");
                    return Ok(None);
                }
            };

            // Check for M! prefix (filter out non-message data)
            let json_str = if message_str.starts_with("M!") {
                // Extract JSON portion (after "M!")
                &message_str[2..]
            } else {
                // No M! prefix - this is not a message (debug output, etc.)
                // Skip it and continue reading
                log::trace!("SerialTransport: Skipping non-message line (no M! prefix)");
                return Ok(None);
            };

            // Parse JSON
            match json::from_str::<ClientMessage>(json_str) {
                Ok(msg) => {
                    log::debug!(
                        "SerialTransport: Received message id={} ({} bytes): {}",
                        msg.id,
                        message_bytes.len(),
                        json_str
                    );
                    Ok(Some(msg))
                }
                Err(e) => {
                    // Parse error - ignore with warning (as specified)
                    log::warn!("SerialTransport: Failed to parse JSON message: {e}");
                    Ok(None)
                }
            }
        } else {
            // No complete message yet
            // ... existing logging code ...
            Ok(None)
        }
    }

    // ... existing close method ...
}
```

### 2. Update SerialTransport send method

Update `send` method to add `M!` prefix:

```rust
impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        // Add M! prefix
        let message = format!("M!{}\n", json);
        let message_bytes = message.as_bytes();

        log::debug!(
            "SerialTransport: Sending message id={} ({} bytes): {}",
            msg.id,
            message_bytes.len(),
            json
        );

        // Write message (blocking)
        self.io
            .write(message_bytes)
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;

        log::trace!("SerialTransport: Wrote {} bytes to serial", message_bytes.len());

        Ok(())
    }

    // ... rest of implementation ...
}
```

### 3. Update tests

Update tests in `lp-fw/fw-core/src/transport/serial.rs`:

```rust
#[test]
fn test_send_message_with_prefix() {
    let mock_io = MockSerialIo::new();
    let mut transport = SerialTransport::new(mock_io);

    let msg = ServerMessage {
        id: 1,
        msg: lp_model::server::ServerMsgBody::UnloadProject,
    };
    transport.send(msg).unwrap();

    let written = transport.io.take_written();
    let written_str = str::from_utf8(&written).unwrap();
    assert!(written_str.starts_with("M!"));
    assert!(written_str.contains("\"unloadProject\""));
    assert!(written_str.ends_with('\n'));
}

#[test]
fn test_receive_message_with_prefix() {
    let mock_io = MockSerialIo::new();
    let mut transport = SerialTransport::new(mock_io);

    let client_msg = ClientMessage {
        id: 1,
        msg: ClientRequest::ListLoadedProjects,
    };
    let json = json::to_string(&client_msg).unwrap();
    let mut msg_bytes = format!("M!{}\n", json).into_bytes();

    transport.io.push_read(&msg_bytes);

    let received = transport.receive().unwrap();
    assert!(received.is_some());
    let received_msg = received.unwrap();
    assert_eq!(received_msg.id, 1);
    assert!(matches!(
        received_msg.msg,
        ClientRequest::ListLoadedProjects
    ));
}

#[test]
fn test_receive_skips_non_message_lines() {
    let mock_io = MockSerialIo::new();
    let mut transport = SerialTransport::new(mock_io);

    // Push debug output (no M! prefix)
    let debug_output = b"debug output line\n";
    transport.io.push_read(debug_output);

    // Should return None (not a message)
    let received = transport.receive().unwrap();
    assert!(received.is_none());
}

#[test]
fn test_receive_message_after_debug_output() {
    let mock_io = MockSerialIo::new();
    let mut transport = SerialTransport::new(mock_io);

    // Push debug output followed by message
    let debug_output = b"debug output\n";
    let client_msg = ClientMessage {
        id: 1,
        msg: ClientRequest::ListLoadedProjects,
    };
    let json = json::to_string(&client_msg).unwrap();
    let mut msg_bytes = format!("M!{}\n", json).into_bytes();
    
    let mut combined = debug_output.to_vec();
    combined.extend_from_slice(&msg_bytes);
    transport.io.push_read(&combined);

    // First receive should skip debug output
    let received1 = transport.receive().unwrap();
    assert!(received1.is_none());

    // Second receive should get the message
    let received2 = transport.receive().unwrap();
    assert!(received2.is_some());
    assert_eq!(received2.unwrap().id, 1);
}
```

## Tests to Write

- Test that `send()` adds `M!` prefix
- Test that `receive()` accepts messages with `M!` prefix
- Test that `receive()` skips lines without `M!` prefix
- Test that messages after debug output are still received correctly
- Test that existing functionality still works (backward compatibility)

## Validate

Run from `lp-fw/fw-core/` directory:

```bash
cd lp-fw/fw-core
cargo test --package fw-core
cargo check --package fw-core
```

Ensure:
- All tests pass
- No warnings
- Code compiles for both `std` and `no_std` targets
- `M!` prefix is added to outgoing messages
- Non-`M!` lines are filtered out
- Existing SerialTransport functionality still works

Note: This change is backward compatible - old code without `M!` prefix will still work (messages just won't be received, which is expected behavior for filtering).
