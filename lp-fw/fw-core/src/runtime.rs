//! Shared firmware runtime loop helpers.
//!
//! Target crates still own boot, hardware setup, scheduling, and yielding. This
//! module only provides the target-neutral parts of a LightPlayer firmware loop:
//! draining client messages and ticking `LpServer` through a `ServerTransport`.

extern crate alloc;

use alloc::vec::Vec;

use lpa_server::{LpServer, ServerError};
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{TransportError, WireMessage};

/// Result of draining currently available client messages from a transport.
#[derive(Debug)]
pub struct DrainedClientMessages {
    pub messages: Vec<WireMessage>,
    pub receive_calls: u32,
    pub error: Option<TransportError>,
}

impl DrainedClientMessages {
    #[must_use]
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

/// Result of one server tick/send step.
#[derive(Debug, Clone)]
pub struct ServerTickOutcome {
    pub delta_ms: u32,
    pub response_count: usize,
    pub frame_time_us: u64,
    pub server_error: Option<ServerError>,
}

/// Drain all currently available client messages from `transport`.
///
/// A receive error is returned alongside any messages already collected. This
/// lets target loops decide whether a specific error is fatal.
pub async fn drain_client_messages<T: ServerTransport>(transport: &mut T) -> DrainedClientMessages {
    let mut messages = Vec::new();
    let mut receive_calls = 0;

    loop {
        receive_calls += 1;
        match transport.receive().await {
            Ok(Some(msg)) => messages.push(WireMessage::Client(msg)),
            Ok(None) => {
                return DrainedClientMessages {
                    messages,
                    receive_calls,
                    error: None,
                };
            }
            Err(error) => {
                return DrainedClientMessages {
                    messages,
                    receive_calls,
                    error: Some(error),
                };
            }
        }
    }
}

/// Tick the server, send responses through `transport`, and record frame time.
pub async fn tick_server_frame<T, P>(
    server: &mut LpServer,
    transport: &mut T,
    time_provider: &P,
    frame_start_ms: u64,
    last_tick_ms: u64,
    incoming_messages: Vec<WireMessage>,
) -> ServerTickOutcome
where
    T: ServerTransport,
    P: TimeProvider,
{
    let delta_time = time_provider.elapsed_ms(last_tick_ms);
    let delta_ms = delta_time.min(u32::MAX as u64) as u32;
    let delta_ms = delta_ms.max(1);

    match server
        .tick_and_send(delta_ms, incoming_messages, transport)
        .await
    {
        Ok(response_count) => {
            let frame_time_us = elapsed_us(time_provider, frame_start_ms);
            server.set_last_frame_time(frame_time_us);
            ServerTickOutcome {
                delta_ms,
                response_count,
                frame_time_us,
                server_error: None,
            }
        }
        Err(error) => {
            let frame_time_us = elapsed_us(time_provider, frame_start_ms);
            server.set_last_frame_time(frame_time_us);
            ServerTickOutcome {
                delta_ms,
                response_count: 0,
                frame_time_us,
                server_error: Some(error),
            }
        }
    }
}

fn elapsed_us<P: TimeProvider>(time_provider: &P, start_ms: u64) -> u64 {
    time_provider.elapsed_ms(start_ms).saturating_mul(1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;
    use lpc_shared::time::TimeProvider;
    use lpc_wire::{ClientMessage, ClientRequest};

    #[test]
    fn drain_client_messages_collects_until_empty() {
        let mut transport = FakeTransport::new();
        transport.queue_message(ClientMessage {
            id: 1,
            msg: ClientRequest::ListAvailableProjects,
        });
        transport.queue_message(ClientMessage {
            id: 2,
            msg: ClientRequest::ListLoadedProjects,
        });

        let drained = pollster::block_on(drain_client_messages(&mut transport));

        assert_eq!(drained.message_count(), 2);
        assert_eq!(drained.receive_calls, 3);
        assert!(drained.error.is_none());
    }

    #[test]
    fn mock_time_provider_reports_elapsed_ms() {
        let time = MockTimeProvider { now_ms: 42 };

        assert_eq!(time.elapsed_ms(40), 2);
    }

    struct MockTimeProvider {
        now_ms: u64,
    }

    impl TimeProvider for MockTimeProvider {
        fn now_ms(&self) -> u64 {
            self.now_ms
        }
    }
}
