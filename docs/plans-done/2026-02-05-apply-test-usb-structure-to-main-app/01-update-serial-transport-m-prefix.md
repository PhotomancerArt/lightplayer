# Phase 1: Update SerialTransport to Support M! Prefix

## Scope of Phase

Update `SerialTransport` in `fw-core` to support the `M!` prefix pattern. This allows filtering out non-message data (debug prints, log output) and makes the transport more robust. This change affects `fw-emu` which uses `SerialTransport`, ensuring consistency across all transports.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update SerialTransport send method

Add `M!` prefix when sending messages:

```rust
impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        // Add M! prefix
        let message = format!("M!{json}\n");
        let message_bytes = message.as_bytes();

        log::debug!(
            "SerialTransport: Sending message id={} ({} bytes): M!{}",
            msg.id,
            message_bytes.len(),
            json
        );

        // Write message with prefix
        self.io
            .write(message_bytes)
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;

        log::trace!("SerialTransport: Wrote {} bytes to serial", message_bytes.len());

        Ok(())
    }
    // ...
}
```

### 2. Update SerialTransport receive method

Filter out lines that don't start with `M!` prefix:

```rust
fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
    // Read available bytes in a loop until we have a complete message or no more data
    let mut temp_buf = [0u8; 256];
    loop {
        match self.io.read_available(&mut temp_buf) {
            Ok(n) => {
                if n > 0 {
                    log::trace!("SerialTransport: Read {n} bytes from serial");
                    self.read_buffer.extend_from_slice(&temp_buf[..n]);
                } else {
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
        // Extract message (without \n)
        let message_bytes: Vec<u8> = self.read_buffer.drain(..=newline_pos).collect();
        let message_str = match str::from_utf8(&message_bytes[..message_bytes.len() - 1]) {
            Ok(s) => s,
            Err(_) => {
                log::warn!("SerialTransport: Invalid UTF-8 in message");
                return Ok(None);
            }
        };

        // Check for M! prefix
        if !message_str.starts_with("M!") {
            // Not a message - skip (likely debug output or log)
            log::trace!("SerialTransport: Skipping non-message line (no M! prefix)");
            return Ok(None);
        }

        // Extract JSON (skip M! prefix)
        let json_str = &message_str[2..];

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
                log::warn!("SerialTransport: Failed to parse JSON message: {e}");
                Ok(None)
            }
        }
    } else {
        // No complete message yet
        Ok(None)
    }
}
```

### 3. Update tests

Update existing tests to expect `M!` prefix:

```rust
#[test]
fn test_send_message() {
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
```

Add test for filtering non-message lines:

```rust
#[test]
fn test_receive_filters_non_message_lines() {
    let mock_io = MockSerialIo::new();
    let mut transport = SerialTransport::new(mock_io);

    // Push a non-message line (no M! prefix)
    mock_io.push_read(b"debug: some log output\n");

    // Should return None (filtered out)
    assert!(transport.receive().unwrap().is_none());

    // Push a valid message
    mock_io.push_read(b"M!{\"id\":1,\"msg\":{\"loadProject\":{\"path\":\"test\"}}}\n");

    // Should parse successfully
    let msg = transport.receive().unwrap().unwrap();
    assert_eq!(msg.id, 1);
}
```

## Validate

Run the following commands to validate:

```bash
# Check compilation
cargo check --package fw-core

# Run tests
cargo test --package fw-core transport::serial

# Check fw-emu still compiles (uses SerialTransport)
cargo check --package fw-emu
```

Ensure:
- All existing tests pass
- `fw-emu` still compiles and works
- Messages are sent with `M!` prefix
- Non-message lines are filtered when receiving
