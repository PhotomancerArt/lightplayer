use lpa_client::{ClientOutcome, SharedClientTransport, TokioLpClient};
use lpc_wire::{ClientRequest, WireServerMessage};

use lpa_studio_core::StudioEvent;

use crate::StudioRuntimeError;
use crate::protocol_event::client_event;
pub use crate::protocol_event::inventory_request;

pub struct ClientSessionRuntime {
    client: TokioLpClient,
}

impl ClientSessionRuntime {
    pub fn new(transport: SharedClientTransport) -> Self {
        Self {
            client: TokioLpClient::new_shared(transport).with_heartbeat_display(false),
        }
    }

    pub async fn send_request(
        &mut self,
        request: ClientRequest,
    ) -> Result<ClientExchange, StudioRuntimeError> {
        let outcome = self
            .client
            .send_request(request)
            .await
            .map_err(map_client_error)?;
        Ok(ClientExchange::from(outcome))
    }
}

pub struct ClientExchange {
    pub response: WireServerMessage,
    pub events: Vec<StudioEvent>,
}

impl From<ClientOutcome<WireServerMessage>> for ClientExchange {
    fn from(outcome: ClientOutcome<WireServerMessage>) -> Self {
        Self {
            response: outcome.value,
            events: outcome.events.into_iter().map(client_event).collect(),
        }
    }
}

fn map_client_error(error: impl std::fmt::Display) -> StudioRuntimeError {
    let message = error.to_string();
    if message.starts_with("Transport error:") || message.starts_with("Request timed out") {
        StudioRuntimeError::Transport(message)
    } else {
        StudioRuntimeError::Protocol(message)
    }
}
