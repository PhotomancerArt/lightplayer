use std::time::{Duration, Instant};

use lpa_server::LpServer;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{TransportError, WireMessage};

use crate::HostRuntimeError;

const TARGET_FRAME_TIME_MS: u32 = 16;

pub async fn run_server_loop_async<T: ServerTransport>(
    mut server: LpServer,
    mut transport: T,
) -> Result<(), HostRuntimeError> {
    let mut last_tick = Instant::now();

    loop {
        let frame_start = Instant::now();
        let mut incoming_messages = Vec::new();

        loop {
            match transport.receive().await {
                Ok(Some(client_msg)) => incoming_messages.push(WireMessage::Client(client_msg)),
                Ok(None) => break,
                Err(TransportError::ConnectionLost) => return Ok(()),
                Err(error) => {
                    eprintln!("Host runtime transport error: {error}");
                    break;
                }
            }
        }

        let delta_time = last_tick.elapsed();
        let delta_ms = delta_time.as_millis().min(u32::MAX as u128) as u32;
        let tick_start = Instant::now();

        if let Err(error) = server
            .tick_and_send(delta_ms.max(1), incoming_messages, &mut transport)
            .await
        {
            eprintln!("Host runtime server error: {error}");
        } else {
            let frame_time_us = tick_start.elapsed().as_micros() as u64;
            server.set_last_frame_time(frame_time_us);
        }

        last_tick = frame_start;
        let frame_duration = frame_start.elapsed();
        if frame_duration < Duration::from_millis(TARGET_FRAME_TIME_MS as u64) {
            tokio::time::sleep(Duration::from_millis(TARGET_FRAME_TIME_MS as u64) - frame_duration)
                .await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}
