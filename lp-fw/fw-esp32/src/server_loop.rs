//! Server loop for ESP32 firmware
//!
//! Main async loop that handles hardware I/O and calls lp-server::tick().

extern crate alloc;

use alloc::vec::Vec;
use lp_model::Message;
use lp_server::LpServer;
use lp_shared::time::TimeProvider;
use lp_shared::transport::ServerTransport;

use crate::time::Esp32TimeProvider;

/// FPS logging interval (log every N frames)
const FPS_LOG_INTERVAL: u32 = 60;

/// Run the server loop
///
/// This is the main async loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to Embassy runtime between iterations.
pub async fn run_server_loop<T: ServerTransport>(
    mut server: LpServer,
    mut transport: T,
    time_provider: Esp32TimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();
    let mut frame_count = 0u32;
    let mut fps_last_log_time = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => {
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Transport error - log and continue
                    log::warn!("run_server_loop: Transport error: {:?}", e);
                    break;
                }
            }
        }

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Send responses
                for response in responses {
                    if let Message::Server(server_msg) = response {
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
        frame_count += 1;

        // Log FPS periodically
        if frame_count % FPS_LOG_INTERVAL == 0 {
            let current_time = time_provider.now_ms();
            let elapsed_ms = current_time.saturating_sub(fps_last_log_time);
            if elapsed_ms > 0 {
                let fps = (FPS_LOG_INTERVAL as u64 * 1000) / elapsed_ms;
                log::info!(
                    "FPS: {} (frame_count: {}, elapsed: {}ms)",
                    fps,
                    frame_count,
                    elapsed_ms
                );
                fps_last_log_time = current_time;
            }
        }

        // Yield to Embassy runtime (allows other tasks to run)
        // Use embassy_time::Timer to delay slightly
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
