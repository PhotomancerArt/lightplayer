//! Emulator serial transport factory
//!
//! Creates async serial transport that communicates with firmware running in emulator.
//! The emulator runs on a separate thread that loops continuously.

use log;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_riscv_emu::Riscv32Emulator;
use serde_json;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::{mpsc, oneshot};

#[cfg(test)]
use lp_riscv_emu::Riscv32Emulator as TestRiscv32Emulator;

/// Maximum steps per emulator iteration before yielding
const MAX_STEPS_PER_ITERATION: u64 = 100_000_000;

/// Emulator thread loop
///
/// Runs continuously, processing messages and communicating via serial I/O.
/// This function runs in a separate thread and owns the emulator.
fn emulator_thread_loop(
    emulator: Arc<Mutex<Riscv32Emulator>>,
    mut client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut read_buffer = Vec::new();

    loop {
        // Check for shutdown signal (non-blocking)
        if shutdown_rx.try_recv().is_ok() {
            log::debug!("Emulator thread: Shutdown signal received");
            break;
        }

        // Process incoming client messages (non-blocking)
        while let Ok(msg) = client_rx.try_recv() {
            // Serialize message to JSON
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    log::warn!("Emulator thread: Failed to serialize client message: {e}");
                    continue;
                }
            };

            // Add newline terminator
            let mut data = json.into_bytes();
            data.push(b'\n');

            log::debug!(
                "Emulator thread: Writing client message id={} ({} bytes) to serial",
                msg.id,
                data.len()
            );

            // Write to emulator serial input
            {
                let mut emu = match emulator.lock() {
                    Ok(e) => e,
                    Err(_) => {
                        log::error!("Emulator thread: Failed to lock emulator");
                        break;
                    }
                };
                emu.serial_write(&data);
            }
        }

        // Run emulator until yield
        // Check instruction count before running to detect if we're making progress
        let initial_instruction_count = {
            match emulator.lock() {
                Ok(emu) => emu.get_instruction_count(),
                Err(_) => {
                    log::error!(
                        "Emulator thread: Failed to lock emulator for instruction count check"
                    );
                    break;
                }
            }
        };

        let run_result = {
            let mut emu = match emulator.lock() {
                Ok(e) => e,
                Err(_) => {
                    log::error!("Emulator thread: Failed to lock emulator for run_until_yield");
                    break;
                }
            };
            emu.run_until_yield(MAX_STEPS_PER_ITERATION)
        };

        // Log progress if we ran a significant number of instructions
        let final_instruction_count = match emulator.lock() {
            Ok(emu) => emu.get_instruction_count(),
            Err(_) => initial_instruction_count,
        };
        let steps_executed = final_instruction_count.saturating_sub(initial_instruction_count);
        if steps_executed > 1_000_000 {
            log::debug!("Emulator thread: Executed {steps_executed} instructions before yield");
        }

        match run_result {
            Ok(_) => {
                log::trace!("Emulator thread: Emulator yielded");
            }
            Err(e) => {
                // Log detailed error information
                match &e {
                    lp_riscv_emu::EmulatorError::InstructionLimitExceeded {
                        limit,
                        executed,
                        pc,
                        ..
                    } => {
                        log::error!(
                            "Emulator thread: Instruction limit exceeded! limit={limit}, executed={executed}, pc=0x{pc:x}"
                        );
                        log::error!(
                            "This usually means the firmware is stuck in a loop or taking too long. \
                             Consider reducing scene complexity or increasing MAX_STEPS_PER_ITERATION."
                        );
                    }
                    lp_riscv_emu::EmulatorError::Panic { info, pc, .. } => {
                        log::error!(
                            "Emulator thread: Firmware panic at pc=0x{:x}: {}",
                            pc,
                            info.message
                        );
                    }
                    lp_riscv_emu::EmulatorError::Trap { code, pc, .. } => {
                        log::error!("Emulator thread: Trap at pc=0x{pc:x}: {code:?}");
                    }
                    _ => {
                        log::error!("Emulator thread: Emulator error: {e:?}");
                    }
                }
                // Close server_tx to signal connection lost
                drop(server_tx);
                break;
            }
        }

        // Drain serial output and parse messages
        let output = {
            let mut emu = match emulator.lock() {
                Ok(e) => e,
                Err(_) => {
                    log::error!("Emulator thread: Failed to lock emulator for drain");
                    break;
                }
            };
            emu.drain_serial_output()
        };

        if !output.is_empty() {
            log::trace!(
                "Emulator thread: Drained {} bytes from serial output",
                output.len()
            );
            read_buffer.extend_from_slice(&output);
        }

        // Parse complete messages (newline-terminated)
        while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
            let message_bytes = read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = match std::str::from_utf8(&message_bytes[..message_bytes.len() - 1]) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Emulator thread: Invalid UTF-8 in message: {e}");
                    continue;
                }
            };

            // Parse JSON message
            match serde_json::from_str::<ServerMessage>(message_str) {
                Ok(msg) => {
                    log::debug!(
                        "Emulator thread: Parsed server message id={} ({} bytes)",
                        msg.id,
                        message_bytes.len()
                    );

                    // Send via server_tx
                    if server_tx.send(msg).is_err() {
                        log::debug!("Emulator thread: server_tx closed, exiting");
                        break;
                    }
                }
                Err(e) => {
                    log::warn!("Emulator thread: Failed to parse JSON message: {e}");
                    // Continue - don't crash on parse errors
                }
            }
        }
    }

    log::debug!("Emulator thread: Exiting");
}

/// Create emulator serial transport pair
///
/// Creates an async serial transport that communicates with firmware running in emulator.
/// The emulator runs on a separate thread that loops continuously.
///
/// # Arguments
///
/// * `emulator` - Shared reference to the emulator (will be moved to thread)
///
/// # Returns
///
/// * `Ok(AsyncSerialClientTransport)` - The async serial transport
/// * `Err(TransportError)` - If channel creation or thread spawning fails
pub fn create_emulator_serial_transport_pair(
    emulator: Arc<Mutex<Riscv32Emulator>>,
) -> Result<super::AsyncSerialClientTransport, TransportError> {
    use super::AsyncSerialClientTransport;

    // Create channels for bidirectional communication
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Spawn emulator thread
    let thread_handle = thread::Builder::new()
        .name("lp-emulator-serial".to_string())
        .spawn(move || {
            emulator_thread_loop(emulator, client_rx, server_tx, shutdown_rx);
        })
        .map_err(|e| TransportError::Other(format!("Failed to spawn emulator thread: {e}")))?;

    Ok(AsyncSerialClientTransport::new(
        client_tx,
        server_rx,
        shutdown_tx,
        thread_handle,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ClientTransport;

    #[tokio::test]
    async fn test_create_transport() {
        // Create a dummy emulator for testing
        let emulator = Arc::new(Mutex::new(TestRiscv32Emulator::new(vec![], vec![])));

        let result = create_emulator_serial_transport_pair(emulator);
        assert!(result.is_ok());

        let mut transport = result.unwrap();

        // Verify transport was created successfully
        // Close it to clean up the thread
        transport.close().await.unwrap();
    }
}
