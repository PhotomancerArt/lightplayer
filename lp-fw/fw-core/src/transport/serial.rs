//! Serial transport implementation using SerialIo
//!
//! Handles message framing (JSON + `\n` termination), buffering partial reads,
//! and JSON parsing. Implements `ServerTransport` trait.

extern crate alloc;

use alloc::{format, vec::Vec};
use core::str;

use crate::serial::SerialIo;
use log;
use lp_model::{ClientMessage, ServerMessage, TransportError};
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

impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        log::debug!("SerialTransport: Sending message id={}", msg.id);

        // Serialize to JSON
        let json = serde_json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        let json_bytes = json.as_bytes();
        log::trace!(
            "SerialTransport: Serialized message to {} bytes JSON",
            json_bytes.len()
        );

        // Write JSON + newline (blocking)
        self.io
            .write(json_bytes)
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;
        self.io
            .write(b"\n")
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;

        log::trace!(
            "SerialTransport: Wrote {} bytes to serial",
            json_bytes.len() + 1
        );

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Read available bytes in a loop until we have a complete message or no more data
        let mut temp_buf = [0u8; 256];
        loop {
            match self.io.read_available(&mut temp_buf) {
                Ok(n) => {
                    if n > 0 {
                        log::trace!("SerialTransport: Read {} bytes from serial", n);
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
                    log::warn!("SerialTransport: Serial read error: {}", e);
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

            // Parse JSON
            match serde_json::from_str::<ClientMessage>(message_str) {
                Ok(msg) => {
                    log::debug!(
                        "SerialTransport: Parsed message id={}, size={} bytes",
                        msg.id,
                        message_bytes.len()
                    );
                    Ok(Some(msg))
                }
                Err(e) => {
                    // Parse error - ignore with warning (as specified)
                    log::warn!("SerialTransport: Failed to parse JSON message: {}", e);
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
                    .map(|b| alloc::format!("{:02x}", b))
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
                    Err(_) => alloc::string::String::from("(invalid UTF-8)")
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

    fn close(&mut self) -> Result<(), TransportError> {
        // Clear read buffer
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
        transport.send(msg).unwrap();

        let written = transport.io.take_written();
        let written_str = str::from_utf8(&written).unwrap();
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
        let json = serde_json::to_string(&client_msg).unwrap();
        let mut msg_bytes = json.as_bytes().to_vec();
        msg_bytes.push(b'\n');

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
    fn test_receive_partial_message() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        let json = serde_json::to_string(&client_msg).unwrap();
        let partial = &json.as_bytes()[..json.len() / 2];

        transport.io.push_read(partial);

        let received = transport.receive().unwrap();
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
        let json1 = serde_json::to_string(&msg1).unwrap();
        let json2 = serde_json::to_string(&msg2).unwrap();
        let mut combined = json1.as_bytes().to_vec();
        combined.push(b'\n');
        combined.extend_from_slice(json2.as_bytes());
        combined.push(b'\n');

        transport.io.push_read(&combined);

        let received1 = transport.receive().unwrap();
        assert!(received1.is_some());
        assert_eq!(received1.unwrap().id, 1);

        let received2 = transport.receive().unwrap();
        assert!(received2.is_some());
        assert_eq!(received2.unwrap().id, 2);
    }

    #[test]
    fn test_receive_invalid_json() {
        let mock_io = MockSerialIo::new();
        let mut transport = SerialTransport::new(mock_io);

        let invalid_json = b"invalid json\n";
        transport.io.push_read(invalid_json);

        // Should return None (parse error ignored)
        let received = transport.receive().unwrap();
        assert!(received.is_none());
    }
}
