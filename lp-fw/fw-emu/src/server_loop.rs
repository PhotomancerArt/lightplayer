//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and calls lp-server::tick().

use crate::serial::SyscallSerialIo;
use crate::time::SyscallTimeProvider;
use alloc::vec::Vec;
use fw_core::transport::SerialTransport;
use lp_model::Message;
use lp_riscv_emu_guest::sys_yield;
use lp_server::LpServer;
use lp_shared::time::TimeProvider;
use lp_shared::transport::ServerTransport;
use log;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to host after each tick using SYSCALL_YIELD.
pub fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<SyscallSerialIo>,
    time_provider: SyscallTimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();
        
        log::debug!(
            "run_server_loop: Starting server loop iteration (time: {}ms)",
            frame_start
        );

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        let mut receive_calls = 0;
        loop {
            receive_calls += 1;
            match transport.receive() {
                Ok(Some(msg)) => {
                    log::debug!(
                        "run_server_loop: Received message id={} on receive call #{}",
                        msg.id,
                        receive_calls
                    );
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    if receive_calls > 1 {
                        log::trace!(
                            "run_server_loop: No more messages after {} receive calls",
                            receive_calls
                        );
                    }
                    // No more messages available
                    break;
                }
                Err(e) => {
                    log::warn!("run_server_loop: Transport error: {:?}", e);
                    // Transport error - break and continue
                    break;
                }
            }
        }
        log::trace!(
            "run_server_loop: Collected {} messages this loop iteration",
            incoming_messages.len()
        );

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                log::trace!(
                    "run_server_loop: Server tick produced {} responses",
                    responses.len()
                );
                // Send responses
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        log::debug!(
                            "run_server_loop: Sending response message id={}",
                            server_msg.id
                        );
                        if let Err(e) = transport.send(server_msg) {
                            log::warn!("run_server_loop: Failed to send response: {:?}", e);
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("run_server_loop: Server tick error: {:?}", e);
                // Server error - continue
            }
        }

        last_tick = frame_start;

        // Yield control back to host
        // This allows the host to process serial output, update time, add serial input, etc.
        sys_yield();
    }
}
