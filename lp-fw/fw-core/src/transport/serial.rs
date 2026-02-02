//! Serial transport implementation using SerialIo
//!
//! Handles message framing (JSON + `\n` termination), buffering partial reads,
//! and JSON parsing. Implements `ServerTransport` trait.

extern crate alloc;

use alloc::{format, vec::Vec};
use core::str;

use crate::serial::SerialIo;
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
        // Serialize to JSON
        let json = serde_json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        let json_bytes = json.as_bytes();

        // Write JSON + newline (blocking)
        self.io
            .write(json_bytes)
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;
        self.io
            .write(b"\n")
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Read available bytes (non-blocking)
        let mut temp_buf = [0u8; 256];
        match self.io.read_available(&mut temp_buf) {
            Ok(n) if n > 0 => {
                // Append to read buffer
                self.read_buffer.extend_from_slice(&temp_buf[..n]);
            }
            Ok(_) => {
                // No data available
            }
            Err(e) => {
                return Err(TransportError::Other(format!("Serial read error: {e}")));
            }
        }

        // Look for complete message (ends with \n)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            // Extract message (without \n)
            let message_bytes: Vec<u8> = self.read_buffer.drain(..=newline_pos).collect();
            let message_str = match str::from_utf8(&message_bytes[..message_bytes.len() - 1]) {
                Ok(s) => s,
                Err(_) => {
                    // Invalid UTF-8, ignore with warning
                    // In no_std, we can't easily log, so just return None
                    return Ok(None);
                }
            };

            // Parse JSON
            match serde_json::from_str::<ClientMessage>(message_str) {
                Ok(msg) => Ok(Some(msg)),
                Err(_) => {
                    // Parse error - ignore with warning (as specified)
                    // In no_std, we can't easily log, so just return None
                    Ok(None)
                }
            }
        } else {
            // No complete message yet
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
