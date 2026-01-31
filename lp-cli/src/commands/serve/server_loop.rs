//! Server main loop
//!
//! Handles the main server loop that processes messages and routes responses.

use anyhow::Result;
use lp_model::{Message, TransportError};
use lp_server::LpServer;
use lp_shared::transport::ServerTransport;
use std::time::{Duration, Instant};

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server main loop
///
/// Processes incoming messages from clients and routes responses back.
/// Ticks continuously at ~60 FPS to advance frames regardless of message activity.
/// This function accepts `LpServer` and transport as parameters for testability.
///
/// # Arguments
///
/// * `server` - The LpServer instance
/// * `transport` - The server transport (handles connections)
pub fn run_server_loop<T: ServerTransport>(mut server: LpServer, mut transport: T) -> Result<()> {
    let mut last_tick = Instant::now();

    // Main server loop - runs at ~60 FPS
    loop {
        let frame_start = Instant::now();

        // Collect incoming messages from all connections (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(client_msg)) => {
                    // Wrap in Message envelope
                    incoming_messages.push(Message::Client(client_msg));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Connection lost is expected when client disconnects - exit gracefully
                    if matches!(e, TransportError::ConnectionLost) {
                        return Ok(());
                    }
                    // Other transport errors - log and continue
                    eprintln!("Transport error: {e}");
                    break;
                }
            }
        }

        // Calculate delta time since last tick
        let delta_time = last_tick.elapsed();
        let delta_ms = delta_time.as_millis().min(u32::MAX as u128) as u32;

        // Measure frame processing time
        let tick_start = Instant::now();

        // Always tick the server to advance frames, even if there are no messages
        // This ensures continuous frame progression at ~60 FPS
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Record frame processing time (in microseconds)
                let frame_time_us = tick_start.elapsed().as_micros() as u64;
                server.set_last_frame_time(frame_time_us);

                // Send responses back via transport
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        if let Err(e) = transport.send(server_msg) {
                            eprintln!("Failed to send response: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Server error: {e}");
                // Continue running despite errors
            }
        }

        last_tick = frame_start;

        // Sleep to maintain ~60 FPS
        // Calculate how long to sleep to hit target frame time
        let frame_duration = frame_start.elapsed();
        if frame_duration < Duration::from_millis(TARGET_FRAME_TIME_MS as u64) {
            let sleep_duration =
                Duration::from_millis(TARGET_FRAME_TIME_MS as u64) - frame_duration;
            std::thread::sleep(sleep_duration);
        } else {
            // Frame took longer than target - yield to avoid busy-waiting
            std::thread::yield_now();
        }
    }
}
