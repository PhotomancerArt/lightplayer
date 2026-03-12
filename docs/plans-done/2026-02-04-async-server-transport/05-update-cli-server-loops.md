# Phase 5: Update CLI Server Loops

## Scope of phase

Update both async and sync CLI server loops to use async `ServerTransport`. The async loop can use `.await` directly, while the sync loop uses `block_on`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-cli/src/server/run_server_loop_async.rs`

Update to use async transport with `.await`:

```rust
use lp_model::{Message, TransportError};
use lp_server::LpServer;
use lp_shared::transport::ServerTransport;
use std::time::{Duration, Instant};

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server main loop asynchronously
///
/// Processes incoming messages from clients and routes responses back.
/// Ticks continuously at ~60 FPS to advance frames regardless of message activity.
/// This is the async version that works with tokio runtime.
pub async fn run_server_loop_async<T: ServerTransport>(
    mut server: LpServer,
    mut transport: T,
) -> anyhow::Result<()> {
    let mut last_tick = Instant::now();

    // Main server loop - runs at ~60 FPS
    loop {
        let frame_start = Instant::now();

        // Collect incoming messages from all connections (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive().await {
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
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Record frame processing time (in microseconds)
                let frame_time_us = tick_start.elapsed().as_micros() as u64;
                server.set_last_frame_time(frame_time_us);

                // Send responses back via transport
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        if let Err(e) = transport.send(server_msg).await {
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
        let frame_duration = frame_start.elapsed();
        if frame_duration < Duration::from_millis(TARGET_FRAME_TIME_MS as u64) {
            let sleep_duration =
                Duration::from_millis(TARGET_FRAME_TIME_MS as u64) - frame_duration;
            tokio::time::sleep(sleep_duration).await;
        } else {
            // Frame took longer than target - yield to avoid busy-waiting
            tokio::task::yield_now().await;
        }
    }
}
```

**Key changes:**
- `transport.receive()` uses `.await`
- `transport.send()` uses `.await`
- All async operations properly awaited

### 2. Update `lp-cli/src/commands/serve/server_loop.rs`

Update sync server loop to use `block_on`:

```rust
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
/// Uses `block_on` to call async transport (safe in sync context).
pub fn run_server_loop<T: ServerTransport>(mut server: LpServer, mut transport: T) -> Result<()> {
    let mut last_tick = Instant::now();

    // Main server loop - runs at ~60 FPS
    loop {
        let frame_start = Instant::now();

        // Collect incoming messages from all connections (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            // Use tokio::runtime::Handle::current() or create runtime for block_on
            // For sync context, we'll use tokio's block_on
            match tokio::runtime::Handle::try_current()
                .map(|handle| handle.block_on(transport.receive()))
                .unwrap_or_else(|_| {
                    // No runtime available, create one
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(transport.receive())
                }) {
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
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Record frame processing time (in microseconds)
                let frame_time_us = tick_start.elapsed().as_micros() as u64;
                server.set_last_frame_time(frame_time_us);

                // Send responses back via transport using block_on
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        let send_result = tokio::runtime::Handle::try_current()
                            .map(|handle| handle.block_on(transport.send(server_msg)))
                            .unwrap_or_else(|_| {
                                tokio::runtime::Runtime::new()
                                    .unwrap()
                                    .block_on(transport.send(server_msg))
                            });
                        if let Err(e) = send_result {
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
```

**Key changes:**
- Uses `tokio::runtime::Handle::current()` or creates runtime for `block_on`
- Calls async transport methods with `block_on`
- Safe in sync context (no deadlock risk)

**Note:** The `block_on` approach may need refinement based on how the sync server loop is called. If it's already in a tokio runtime context, we can use `Handle::current()`. Otherwise, we may need a different approach.

## Tests

Update tests to use async transport:

```rust
#[tokio::test]
async fn test_async_server_loop() {
    // Test async server loop
}

#[test]
fn test_sync_server_loop() {
    // Test sync server loop with block_on
}
```

## Validate

Run:
```bash
cd lp-cli
cargo check
```

**Expected:** Code compiles. Both async and sync CLI server loops should work with async transport.
