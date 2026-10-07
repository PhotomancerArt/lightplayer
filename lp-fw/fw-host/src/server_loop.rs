use std::sync::Arc;
use std::time::{Duration, Instant};

use fw_core::{drain_client_messages, send_unsolicited_hello, tick_server_frame};
use lpa_server::LpServer;
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::TransportError;
use tokio::sync::Notify;

use crate::HostRuntimeError;

const TARGET_FRAME_TIME_MS: u32 = 16;

/// Serve `transport` at one frame per [`TARGET_FRAME_TIME_MS`], ending a
/// frame's idle wait early when `wake` is notified — a request arrived, and
/// the board answers it on its next tick rather than a frame later.
pub async fn run_server_loop_async<T: ServerTransport>(
    mut server: LpServer,
    mut transport: T,
    wake: Arc<Notify>,
) -> Result<(), HostRuntimeError> {
    let time_provider = HostLoopTimeProvider::new();
    let mut last_tick_ms = time_provider.now_ms();

    // Wire hello: the first id-0 frame this loop ever sends (see
    // docs/adr/2026-07-14-wire-hello-versioning.md).
    if let Err(error) = send_unsolicited_hello(&server, &mut transport).await {
        match error {
            TransportError::ConnectionLost => return Ok(()),
            error => eprintln!("Host runtime hello send error: {error}"),
        }
    }

    loop {
        let frame_start = Instant::now();
        let frame_start_ms = time_provider.now_ms();
        let drained = drain_client_messages(&mut transport).await;
        if let Some(error) = drained.error {
            match error {
                TransportError::ConnectionLost => return Ok(()),
                error => eprintln!("Host runtime transport error: {error}"),
            }
        }

        let tick = tick_server_frame(
            &mut server,
            &mut transport,
            &time_provider,
            frame_start_ms,
            last_tick_ms,
            drained.messages,
        )
        .await;
        if let Some(error) = tick.server_error {
            eprintln!("Host runtime server error: {error}");
        }

        last_tick_ms = frame_start_ms;
        let frame_duration = frame_start.elapsed();
        if frame_duration < Duration::from_millis(TARGET_FRAME_TIME_MS as u64) {
            let idle = Duration::from_millis(TARGET_FRAME_TIME_MS as u64) - frame_duration;
            // Elapsed or woken: either way the next frame starts now.
            let _ = tokio::time::timeout(idle, wake.notified()).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}

struct HostLoopTimeProvider {
    start: Instant,
}

impl HostLoopTimeProvider {
    fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl TimeProvider for HostLoopTimeProvider {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}
