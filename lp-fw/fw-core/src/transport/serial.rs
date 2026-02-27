//! Serial transport implementation using SerialIo
//!
//! Handles message framing (JSON + `\n` termination), buffering partial reads,
//! and JSON parsing. Implements `ServerTransport` trait.
//!
//! When `emu` feature is enabled, uses ser-write-json for streaming serialization
//! (same as fw-esp32) instead of buffering full JSON.

extern crate alloc;

use alloc::{format, vec::Vec};
use core::str;

#[cfg(feature = "emu")]
use crate::serial::SerialError;
use crate::serial::SerialIo;
use log;
use lp_model::{ClientMessage, ServerMessage, TransportError, json};
use lp_shared::transport::ServerTransport;

/// Serial transport implementation
///
/// Uses `SerialIo` for raw byte I/O and handles message framing, buffering,
/// and JSON parsing internally.
pub struct SerialTransport<Io: SerialIo> {
    /// Serial I/O implementation
    io: Io,
    /// Buffer for partial reads (until we get a complete message)
    read_buffer: Vec<u8>,
}

impl<Io: SerialIo> SerialTransport<Io> {
    /// Create a new serial transport with the given SerialIo implementation
    pub fn new(io: Io) -> Self {
        Self {
            io,
            read_buffer: Vec::new(),
        }
    }
}

/// SerWrite adapter for SerialIo - streams bytes to serial without buffering full JSON
#[cfg(feature = "emu")]
struct SerialIoSerWrite<'a, Io: SerialIo>(&'a mut Io);

#[cfg(feature = "emu")]
impl<Io: SerialIo> ser_write_json::SerWrite for SerialIoSerWrite<'_, Io> {
    type Error = SerialError;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0.write(buf)
    }
}

impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        let id = msg.id;

        #[cfg(feature = "emu")]
        {
            // Stream JSON via ser-write-json (same as fw-esp32, no full buffer)
            self.io
                .write(b"M!")
                .map_err(|e| TransportError::Other(alloc::format!("Serial write error: {e}")))?;
            let mut writer = SerialIoSerWrite(&mut self.io);
            ser_write_json::ser::to_writer(&mut writer, &msg).map_err(|e| {
                TransportError::Serialization(alloc::format!(
                    "Failed to serialize ServerMessage: {e}"
                ))
            })?;
            self.io
                .write(b"\n")
                .map_err(|e| TransportError::Other(alloc::format!("Serial write error: {e}")))?;
            log::debug!(
                "SerialTransport: Sent message id={} via ser-write-json (streaming)",
                id
            );
        }

        #[cfg(not(feature = "emu"))]
        {
            // Buffered serialization (legacy path)
            let json = json::to_string(&msg).map_err(|e| {
                TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
            })?;
            let message = format!("M!{json}\n");
            let message_bytes = message.as_bytes();
            log::debug!(
                "SerialTransport: Sending message id={} ({} bytes): M!{}",
                id,
                message_bytes.len(),
                json
            );
            self.io
                .write(message_bytes)
                .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;
        }

        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
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
                    // Parse error - ignore with warning (as specified)
                    log::warn!(
                        "SerialTransport: Failed to parse JSON message: {e} | json: {json_str}"
                    );
                    Ok(None)
                }
            }
        } else {
            // No complete message yet
            // Log buffer contents (first 100 bytes as hex, first 50 bytes as string if valid UTF-8)
            let preview_len = self.read_buffer.len().min(100);
            let hex_preview = if preview_len > 0 {
                self.read_buffer[..preview_len]
                    .iter()
                    .take(50) // Limit hex output to first 50 bytes
                    .map(|b| alloc::format!("{b:02x}"))
                    .collect::<alloc::vec::Vec<_>>()
                    .join(" ")
            } else {
                alloc::string::String::from("(empty)")
            };

            let string_preview = if preview_len > 0 {
                match core::str::from_utf8(&self.read_buffer[..preview_len.min(50)]) {
                    Ok(s) => {
                        // Convert &str to String in no_std, escape control chars
                        let mut result = alloc::string::String::new();
                        for ch in s.chars() {
                            if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
                                result.push_str(&alloc::format!("\\x{:02x}", ch as u8));
                            } else {
                                result.push(ch);
                            }
                        }
                        result
                    }
                    Err(_) => alloc::string::String::from("(invalid UTF-8)"),
                }
            } else {
                alloc::string::String::from("(empty)")
            };

            if self.read_buffer.len() > 100 {
                log::trace!(
                    "SerialTransport: No complete message yet ({} bytes buffered) hex[0..50]: {}, str[0..50]: '{}'... (truncated)",
                    self.read_buffer.len(),
                    hex_preview,
                    string_preview
                );
            } else {
                log::trace!(
                    "SerialTransport: No complete message yet ({} bytes buffered) hex: {}, str: '{}'",
                    self.read_buffer.len(),
                    hex_preview,
                    string_preview
                );
            }
            Ok(None)
        }
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        let mut messages = Vec::new();
        loop {
            match self.receive().await? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        Ok(messages)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.read_buffer.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use crate::serial::SerialError;
    use alloc::vec::Vec;
    use core::{cell::RefCell, str};
    use lp_model::ClientRequest;

    // Mock SerialIo for testing
    struct MockSerialIo {
        read_data: RefCell<Vec<u8>>,
        write_data: RefCell<Vec<u8>>,
    }

    impl MockSerialIo {
        fn new() -> Self {
            Self {
                read_data: RefCell::new(Vec::new()),
                write_data: RefCell::new(Vec::new()),
            }
        }

        fn push_read(&self, data: &[u8]) {
            self.read_data.borrow_mut().extend_from_slice(data);
        }

        fn take_written(&self) -> Vec<u8> {
            self.write_data.borrow_mut().drain(..).collect()
        }
    }

    impl SerialIo for MockSerialIo {
        fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
            self.write_data.borrow_mut().extend_from_slice(data);
            Ok(())
        }

        fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
            let mut read_buf = self.read_data.borrow_mut();
            let to_read = read_buf.len().min(buf.len());
            if to_read > 0 {
                buf[..to_read].copy_from_slice(&read_buf[..to_read]);
                read_buf.drain(..to_read);
            }
            Ok(to_read)
        }

        fn has_data(&self) -> bool {
            !self.read_data.borrow().is_empty()
        }
    }

    #[test]
    fn test_send_message() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let msg = ServerMessage {
            id: 1,
            msg: lp_model::server::ServerMsgBody::UnloadProject,
        };
        pollster::block_on(transport.send(msg)).unwrap();

        let written = transport.io.take_written();
        let written_str = str::from_utf8(&written).unwrap();
        assert!(
            written_str.starts_with("M!"),
            "Message should start with M! prefix"
        );
        assert!(written_str.contains("\"unloadProject\""));
        assert!(written_str.ends_with('\n'));
    }

    #[test]
    fn test_receive_complete_message() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        let json = json::to_string(&client_msg).unwrap();
        let msg_bytes = format!("M!{json}\n").into_bytes();

        transport.io.push_read(&msg_bytes);

        let received = pollster::block_on(transport.receive()).unwrap();
        assert!(received.is_some());
        let received_msg = received.unwrap();
        assert_eq!(received_msg.id, 1);
        assert!(matches!(
            received_msg.msg,
            ClientRequest::ListLoadedProjects
        ));
    }

    #[test]
    fn test_receive_partial_message() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        let json = json::to_string(&client_msg).unwrap();
        let partial = &json.as_bytes()[..json.len() / 2];

        transport.io.push_read(partial);

        let received = pollster::block_on(transport.receive()).unwrap();
        assert!(received.is_none());
    }

    #[test]
    fn test_receive_multiple_messages() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let msg1 = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        let msg2 = ClientMessage {
            id: 2,
            msg: ClientRequest::ListAvailableProjects,
        };
        let json1 = json::to_string(&msg1).unwrap();
        let json2 = json::to_string(&msg2).unwrap();
        let mut combined = format!("M!{json1}\n").into_bytes();
        combined.extend_from_slice(format!("M!{json2}\n").as_bytes());

        transport.io.push_read(&combined);

        let received1 = pollster::block_on(transport.receive()).unwrap();
        assert!(received1.is_some());
        assert_eq!(received1.unwrap().id, 1);

        let received2 = pollster::block_on(transport.receive()).unwrap();
        assert!(received2.is_some());
        assert_eq!(received2.unwrap().id, 2);
    }

    #[test]
    fn test_receive_invalid_json() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let invalid_json = b"M!invalid json\n";
        transport.io.push_read(invalid_json);

        // Should return None (parse error ignored)
        let received = pollster::block_on(transport.receive()).unwrap();
        assert!(received.is_none());
    }

    #[test]
    fn test_receive_filters_non_message_lines() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        // Push a non-message line (no M! prefix)
        transport.io.push_read(b"debug: some log output\n");

        // Should return None (filtered out)
        let received = pollster::block_on(transport.receive()).unwrap();
        assert!(
            received.is_none(),
            "Non-message lines should be filtered out"
        );

        // Push a valid message
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        let json = json::to_string(&client_msg).unwrap();
        let msg_bytes = format!("M!{json}\n").into_bytes();
        transport.io.push_read(&msg_bytes);

        // Should parse successfully
        let received = pollster::block_on(transport.receive()).unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().id, 1);
    }
}
