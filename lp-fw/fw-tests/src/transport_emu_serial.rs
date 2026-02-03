//! Serial ClientTransport implementation for emulator (test-specific)
//!
//! Runs the emulator synchronously when sending/receiving messages.
//! When sending a message, runs the emulator until it yields a response.

use async_trait::async_trait;
use log;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_riscv_emu::Riscv32Emulator;
use serde_json;
use std::sync::{Arc, Mutex};

/// Serial ClientTransport that communicates with firmware running in emulator
///
/// Runs the emulator synchronously - when sending a message, runs until yield.
/// This is simpler than async task approach and fails fast if no response.
pub struct SerialEmuClientTransport {
    /// Emulator instance (shared, mutex-protected)
    emulator: Arc<Mutex<Riscv32Emulator>>,
    /// Buffer for partial messages (when reading from serial)
    read_buffer: Vec<u8>,
}

impl SerialEmuClientTransport {
    /// Create a new serial client transport
    ///
    /// # Arguments
    /// * `emulator` - Shared reference to the emulator
    pub fn new(emulator: Arc<Mutex<Riscv32Emulator>>) -> Self {
        Self {
            emulator,
            read_buffer: Vec::new(),
        }
    }

    /// Read a complete JSON message from serial output
    ///
    /// Messages are newline-terminated JSON.
    fn read_message(&mut self) -> Result<Option<ServerMessage>, TransportError> {
        // Drain serial output from emulator
        let output = {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.drain_serial_output()
        };

        if !output.is_empty() {
            log::trace!(
                "SerialEmuClientTransport::read_message: Drained {} bytes from serial output",
                output.len()
            );
            self.read_buffer.extend_from_slice(&output);
        }

        // Look for complete message (newline-terminated)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            let message_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = std::str::from_utf8(&message_bytes[..message_bytes.len() - 1])
                .map_err(|e| TransportError::Serialization(format!("Invalid UTF-8: {e}")))?;

            log::trace!(
                "SerialEmuClientTransport: Parsing message ({} bytes)",
                message_bytes.len()
            );
            log::debug!("SerialEmuClientTransport: Parsing message: {}", message_str);

            let message: ServerMessage = serde_json::from_str(message_str)
                .map_err(|e| TransportError::Serialization(format!("JSON parse error: {e}")))?;

            log::debug!(
                "SerialEmuClientTransport: Received message id={}",
                message.id
            );
            log::trace!(
                "SerialEmuClientTransport: Message size={} bytes",
                message_bytes.len()
            );

            Ok(Some(message))
        } else {
            if !self.read_buffer.is_empty() {
                log::trace!(
                    "SerialEmuClientTransport: Partial message buffered ({} bytes): {:?}",
                    self.read_buffer.len(),
                    String::from_utf8_lossy(&self.read_buffer)
                );
            }
            Ok(None)
        }
    }

    /// Run emulator until yield
    fn run_until_yield(&mut self) -> Result<(), TransportError> {
        const MAX_STEPS_PER_ITERATION: u64 = 100_000_000;

        // Run emulator until yield
        let result = {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.run_until_yield(MAX_STEPS_PER_ITERATION)
        };

        match result {
            Ok(_) => {
                log::trace!("SerialEmuClientTransport: Emulator yielded");
                Ok(())
            }
            Err(e) => {
                // Print emulator state on error for debugging
                if let Ok(emu) = self.emulator.lock() {
                    log::error!("Emulator error in run_until_yield: {:?}", e);
                    log::error!("Emulator state:\n{}", emu.dump_state());
                    log::error!(
                        "Last {} instructions:\n{}",
                        emu.get_logs().len(),
                        emu.format_logs()
                    );
                    if let Some(regs) = e.regs() {
                        log::error!("Registers at error: {:?}", regs);
                    }
                }
                Err(TransportError::Other(format!("Emulator error: {:?}", e)))
            }
        }
    }
}

#[async_trait]
impl lp_client::transport::ClientTransport for SerialEmuClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        log::debug!("SerialEmuClientTransport: Sending message id={}", msg.id);

        // Serialize message to JSON
        let json = serde_json::to_string(&msg)
            .map_err(|e| TransportError::Serialization(format!("JSON serialize error: {e}")))?;

        log::trace!(
            "SerialEmuClientTransport: Serialized message ({} bytes): {}",
            json.len(),
            json
        );

        // Add newline terminator
        let mut data = json.into_bytes();
        data.push(b'\n');

        log::trace!(
            "SerialEmuClientTransport: Writing {} bytes to emulator serial input",
            data.len()
        );

        // Add to emulator's serial input buffer
        {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.serial_write(&data);
        }

        log::trace!("SerialEmuClientTransport: Message written to serial buffer");

        Ok(())
    }

    async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
        log::debug!("SerialEmuClientTransport::receive: Waiting for message");

        // Check if we already have a message buffered
        if let Some(msg) = self.read_message()? {
            log::debug!(
                "SerialEmuClientTransport::receive: Found message in buffer id={}",
                msg.id
            );
            log::trace!(
                "SerialEmuClientTransport::receive: Message content: {}",
                serde_json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
            );
            return Ok(msg);
        }

        // No message available, run emulator until yield
        // The firmware should have processed the message and sent a response
        self.run_until_yield()?;

        // Check for message after yield
        if let Some(msg) = self.read_message()? {
            log::debug!(
                "SerialEmuClientTransport::receive: Found message after yield id={}",
                msg.id
            );
            log::trace!(
                "SerialEmuClientTransport::receive: Message content: {}",
                serde_json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
            );
            return Ok(msg);
        }

        // No message after yield - firmware should have sent response before yielding
        Err(TransportError::Other(
            "Emulator yielded but no response message received".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Nothing to close for emulator transport
        Ok(())
    }
}
