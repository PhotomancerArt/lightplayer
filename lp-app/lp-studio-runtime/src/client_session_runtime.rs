use std::sync::Arc;
use std::time::Duration;

use lpa_client::ClientTransport;
use lpc_wire::{ClientRequest, WireServerMessage, WireServerMsgBody, messages::ClientMessage};
use tokio::sync::Mutex;
use tokio::time::timeout;

use lp_studio_core::{StudioEvent, StudioLogEntry, StudioLogLevel};

use crate::StudioRuntimeError;
pub use crate::protocol_event::inventory_request;
use crate::protocol_event::server_event;

pub type SharedClientTransport = Arc<Mutex<Box<dyn ClientTransport>>>;

pub struct ClientSessionRuntime {
    transport: SharedClientTransport,
    next_request_id: u64,
}

impl ClientSessionRuntime {
    pub fn new(transport: SharedClientTransport) -> Self {
        Self {
            transport,
            next_request_id: 1,
        }
    }

    pub async fn send_request(
        &mut self,
        request: ClientRequest,
    ) -> Result<ClientExchange, StudioRuntimeError> {
        let request_id = self.next_request_id();
        self.send_request_with_id(request_id, request).await
    }

    pub async fn send_request_with_id(
        &mut self,
        request_id: u64,
        request: ClientRequest,
    ) -> Result<ClientExchange, StudioRuntimeError> {
        let msg = ClientMessage {
            id: request_id,
            msg: request,
        };

        let mut transport = self.transport.lock().await;
        transport
            .send(msg)
            .await
            .map_err(|error| StudioRuntimeError::Transport(error.to_string()))?;

        let mut events = Vec::new();
        loop {
            let response = timeout(Duration::from_secs(60), transport.receive())
                .await
                .map_err(|_| StudioRuntimeError::Transport("request timed out".to_string()))?
                .map_err(|error| StudioRuntimeError::Transport(error.to_string()))?;

            if response.id == request_id {
                if let WireServerMsgBody::Error { error } = &response.msg {
                    return Err(StudioRuntimeError::Protocol(error.clone()));
                }
                return Ok(ClientExchange { response, events });
            }

            if response.id == 0 {
                if let Some(event) = server_event(response) {
                    events.push(event);
                }
                continue;
            }

            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(
                    StudioLogLevel::Warn,
                    "lp-studio-runtime",
                    format!(
                        "Ignoring uncorrelated server response id={} while waiting for id={request_id}",
                        response.id
                    ),
                ),
            });
        }
    }

    pub fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }
}

pub struct ClientExchange {
    pub response: WireServerMessage,
    pub events: Vec<StudioEvent>,
}
