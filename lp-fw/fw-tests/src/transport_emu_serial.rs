//! Serial ClientTransport implementation for emulator (test-specific)
//!
//! Runs the emulator synchronously when sending/receiving messages.
//! When sending a message, runs the emulator until it yields a response.

use async_trait::async_trait;
use log;
use lp_model::{ClientMessage, ServerMessage, TransportError, json};
use lp_riscv_emu::Riscv32Emulator;
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

        // Process complete lines (newline-terminated); skip non-M! lines (server logs)
        while let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            let message_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = std::str::from_utf8(&message_bytes[..message_bytes.len() - 1])
                .map_err(|e| TransportError::Serialization(format!("Invalid UTF-8: {e}")))?;

            if !message_str.starts_with("M!") {
                log::trace!(
                    "SerialEmuClientTransport: Skipping non-message line ({} bytes)",
                    message_bytes.len()
                );
                continue;
            }

            let json_str = message_str.strip_prefix("M!").unwrap_or(message_str);
            log::trace!(
                "SerialEmuClientTransport: Parsing message ({} bytes)",
                message_bytes.len()
            );

            match json::from_str::<ServerMessage>(json_str) {
                Ok(message) => {
                    log::debug!(
                        "SerialEmuClientTransport: Received message id={} ({} bytes): {}",
                        message.id,
                        message_bytes.len(),
                        message_str
                    );
                    return Ok(Some(message));
                }
                Err(e) => {
                    log::warn!(
                        "SerialEmuClientTransport: Failed to parse M! line: {e} | {}",
                        message_str
                    );
                }
            }
        }

        if !self.read_buffer.is_empty() {
            log::trace!(
                "SerialEmuClientTransport: Partial message buffered ({} bytes): {:?}",
                self.read_buffer.len(),
                String::from_utf8_lossy(&self.read_buffer)
            );
        }
        Ok(None)
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
                    log::error!("Emulator error in run_until_yield: {e:?}");
                    log::error!("Emulator state:\n{}", emu.dump_state());
                    log::error!(
                        "Last {} instructions:\n{}",
                        emu.get_logs().len(),
                        emu.format_logs()
                    );
                    if let Some(regs) = e.regs() {
                        log::error!("Registers at error: {regs:?}");
                    }
                }
                Err(TransportError::Other(format!("Emulator error: {e:?}")))
            }
        }
    }
}

#[async_trait]
impl lp_client::transport::ClientTransport for SerialEmuClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        // Serialize message to JSON (M! prefix per protocol)
        let json = json::to_string(&msg)
            .map_err(|e| TransportError::Serialization(format!("JSON serialize error: {e}")))?;

        let mut data = b"M!".to_vec();
        data.extend_from_slice(json.as_bytes());
        data.push(b'\n');
        let total_bytes = data.len();

        log::debug!(
            "SerialEmuClientTransport: Sending message id={} ({} bytes): {}",
            msg.id,
            total_bytes,
            json
        );

        log::trace!(
            "SerialEmuClientTransport: Serialized message ({} bytes)",
            json.len()
        );

        log::trace!(
            "SerialEmuClientTransport: Writing {total_bytes} bytes to emulator serial input"
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
                json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
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
                json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
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
