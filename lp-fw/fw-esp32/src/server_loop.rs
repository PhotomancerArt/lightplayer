//! Server loop for ESP32 firmware
//!
//! Main async loop that handles hardware I/O and calls lp-server::tick().
//!
//! # Heartbeat Messages
//!
//! The server loop sends periodic heartbeat messages (every second) containing:
//! - Current FPS (frames per second)
//! - Total frame count since startup
//! - List of loaded projects
//! - Server uptime in milliseconds
//!
//! These messages use `ServerMessage` with `id: 0` to indicate they are unsolicited
//! status updates (not responses to client requests). Clients can subscribe to these
//! messages to monitor server health or ignore them if not needed.
//!
//! See `lp-model/src/server/api.rs` for the `ServerMsgBody::Heartbeat` variant definition.
//!
//! # Prior Art
//!
//! This implementation follows the pattern established in `fw-esp32/src/tests/test_usb.rs`
//! which sends heartbeat messages for debugging. This implementation makes heartbeat messages
//! part of the formal protocol using proper `ServerMessage` types with `M!` prefix.

extern crate alloc;

use alloc::vec::Vec;
use lp_model::Message;
use lp_server::LpServer;
use lp_shared::fps::FpsTracker;
use lp_shared::stats::WindowedStatsCollector;
use lp_shared::time::TimeProvider;
use lp_shared::transport::ServerTransport;

use crate::time::Esp32TimeProvider;

/// FPS logging interval (log every N frames)
const FPS_LOG_INTERVAL: u32 = 60;

/// Heartbeat message interval (send every N milliseconds)
const HEARTBEAT_INTERVAL_MS: u64 = 1000; // Send every second

/// FPS statistics window: stats are computed over samples from the last N milliseconds
const FPS_STATS_WINDOW_MS: u64 = 5000;

/// Special message ID for unsolicited heartbeat messages
///
/// Heartbeat messages are not responses to client requests, so they use id=0
/// to indicate they are unsolicited status updates.
const HEARTBEAT_MESSAGE_ID: u64 = 0;

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
    let mut fps_tracker = FpsTracker::new(time_provider.now_ms());
    let mut heartbeat_last_sent = time_provider.now_ms();
    let startup_time = time_provider.now_ms();
    let mut fps_collector = WindowedStatsCollector::new();

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
                    log::warn!("run_server_loop: Transport error: {e:?}");
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
                            log::warn!("run_server_loop: Failed to send response: {e:?}");
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("run_server_loop: Server tick error: {e:?}");
                // Server error - continue
            }
        }

        last_tick = frame_start;
        frame_count += 1;

        // Log FPS periodically
        let current_time = time_provider.now_ms();
        if frame_count % FPS_LOG_INTERVAL == 0 {
            let elapsed_ms = current_time.saturating_sub(fps_tracker.last_log_time_ms());
            if elapsed_ms > 0 {
                let frames_done = frame_count.saturating_sub(fps_tracker.last_log_frame());
                let fps = (frames_done as u64 * 1000) / elapsed_ms;
                log::info!("FPS: {fps} (frame_count: {frame_count}, elapsed: {elapsed_ms}ms)");
                fps_tracker.record_log(frame_count, current_time);
            }
        }

        // Send heartbeat message periodically
        // See prior art: fw-esp32/src/tests/test_usb.rs heartbeat_task()
        // This implementation uses proper ServerMessage types with M! prefix
        if current_time.saturating_sub(heartbeat_last_sent) >= HEARTBEAT_INTERVAL_MS {
            // Get loaded projects from server
            let loaded_projects = server.project_manager().list_loaded_projects();

            let fps_current = fps_tracker
                .instantaneous_fps(frame_count, current_time, startup_time)
                .unwrap_or(0.0);

            fps_collector.push(current_time, fps_current);
            fps_collector.prune_older_than(current_time.saturating_sub(FPS_STATS_WINDOW_MS));
            let fps_stats = fps_collector.compute_stats();

            // Create heartbeat message
            let heartbeat_msg = lp_model::ServerMessage {
                id: HEARTBEAT_MESSAGE_ID,
                msg: lp_model::server::ServerMsgBody::Heartbeat {
                    fps: fps_stats,
                    frame_count: frame_count as u64,
                    loaded_projects,
                    uptime_ms: current_time.saturating_sub(startup_time),
                },
            };

            // Send heartbeat (non-blocking, ignore errors)
            if let Err(e) = transport.send(heartbeat_msg) {
                log::warn!("run_server_loop: Failed to send heartbeat: {e:?}");
            }

            heartbeat_last_sent = current_time;
        }

        // Yield to Embassy runtime (allows other tasks to run)
        // Use embassy_time::Timer to delay slightly
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
