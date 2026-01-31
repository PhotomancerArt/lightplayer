//! Serial ClientTransport implementation for emulator
//!
//! Bridges async ClientTransport calls to synchronous emulator serial I/O.
//! The emulator should be run in a separate async task using `spawn_emulator_task`.

use async_trait::async_trait;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_riscv_emu::{EmulatorError, Riscv32Emulator};
use serde_json;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// Serial ClientTransport that communicates with firmware running in emulator
///
/// This transport only reads/writes serial messages. The emulator must be run
/// in a separate async task using `spawn_emulator_task`.
pub struct SerialClientTransport {
    /// Emulator instance (shared, mutex-protected)
    emulator: Arc<Mutex<Riscv32Emulator>>,
    /// Buffer for partial messages (when reading from serial)
    read_buffer: Vec<u8>,
    /// Notifier for when emulator yields (allows receive to wait)
    yield_notify: Arc<Notify>,
}

impl SerialClientTransport {
    /// Create a new serial client transport
    ///
    /// # Arguments
    /// * `emulator` - Shared reference to the emulator
    pub fn new(emulator: Arc<Mutex<Riscv32Emulator>>) -> (Self, Arc<Notify>) {
        let yield_notify = Arc::new(Notify::new());
        let transport = Self {
            emulator,
            read_buffer: Vec::new(),
            yield_notify: yield_notify.clone(),
        };
        (transport, yield_notify)
    }

    /// Spawn an async task that runs the emulator in a loop
    ///
    /// This task continuously runs the emulator until yield, then notifies waiting receivers.
    /// It should be spawned before using the transport.
    ///
    /// # Arguments
    /// * `emulator` - Shared reference to the emulator
    /// * `yield_notify` - Notifier to signal when emulator yields
    ///
    /// # Returns
    /// * `tokio::task::JoinHandle` - Handle to the spawned task (can be used to abort it)
    pub fn spawn_emulator_task(
        emulator: Arc<Mutex<Riscv32Emulator>>,
        yield_notify: Arc<Notify>,
    ) -> tokio::task::JoinHandle<Result<(), EmulatorError>> {
        tokio::spawn(async move {
            const MAX_STEPS_PER_ITERATION: u64 = 1_000_000;

            loop {
                // Run emulator until yield
                let result = {
                    let mut emu = emulator.lock().map_err(|_| {
                        EmulatorError::InvalidInstruction {
                            pc: 0,
                            instruction: 0,
                            reason: "Failed to lock emulator".to_string(),
                            regs: [0; 32],
                        }
                    })?;

                    emu.step_until_yield(MAX_STEPS_PER_ITERATION)
                };

                match result {
                    Ok(_) => {
                        // Emulator yielded - notify waiting receivers
                        yield_notify.notify_waiters();
                    }
                    Err(EmulatorError::InstructionLimitExceeded { .. }) => {
                        // Hit step limit - notify anyway and continue
                        yield_notify.notify_waiters();
                        // Yield to async runtime to avoid busy-waiting
                        tokio::task::yield_now().await;
                    }
                    Err(e) => {
                        // Actual error - return it
                        return Err(e);
                    }
                }
            }
        })
    }

    /// Read a complete JSON message from serial output
    ///
    /// Messages are newline-terminated JSON.
    fn read_message(&mut self) -> Result<Option<ServerMessage>, TransportError> {
        let mut emu = self
            .emulator
            .lock()
            .map_err(|_| TransportError::ConnectionLost)?;

        // Drain serial output and append to buffer
        let output = emu.drain_serial_output();
        self.read_buffer.extend_from_slice(&output);

        // Look for complete message (newline-terminated)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            let message_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = std::str::from_utf8(&message_bytes[..message_bytes.len() - 1])
                .map_err(|e| TransportError::Serialization(format!("Invalid UTF-8: {e}")))?;

            let message: ServerMessage = serde_json::from_str(message_str)
                .map_err(|e| TransportError::Serialization(format!("JSON parse error: {e}")))?;

            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl crate::transport::ClientTransport for SerialClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        // Serialize message to JSON
        let json = serde_json::to_string(&msg)
            .map_err(|e| TransportError::Serialization(format!("JSON serialize error: {e}")))?;

        // Add newline terminator
        let mut data = json.into_bytes();
        data.push(b'\n');

        // Add to emulator's serial input buffer
        let mut emu = self
            .emulator
            .lock()
            .map_err(|_| TransportError::ConnectionLost)?;
        emu.serial_write(&data);

        Ok(())
    }

    async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
        // Try reading existing buffer first
        if let Some(msg) = self.read_message()? {
            return Ok(msg);
        }

        // No message available, wait for emulator to yield
        // The emulator task will notify us when it yields
        loop {
            // Wait for yield notification (with timeout to check for messages periodically)
            tokio::select! {
                _ = self.yield_notify.notified() => {
                    // Emulator yielded, check for message
                    if let Some(msg) = self.read_message()? {
                        return Ok(msg);
                    }
                    // No message yet, wait for next yield
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    // Periodic check for messages (in case emulator already produced output)
                    if let Some(msg) = self.read_message()? {
                        return Ok(msg);
                    }
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Nothing to close for emulator transport
        Ok(())
    }
}
