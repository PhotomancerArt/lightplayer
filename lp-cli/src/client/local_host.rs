//! CLI local-host transport.
//!
//! This module adapts `lpa-link`'s `local-host` provider to the CLI's current
//! `ClientTransport` return shape while keeping the link session alive for the
//! lifetime of the transport.

use anyhow::Result;
use lpa_client::ClientTransport;
use lpa_link::providers::local_host::{LocalHostProvider, LocalHostSession};
use lpa_link::{LinkError, LinkProvider, LinkSession};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Client transport backed by an in-process `fw-host` runtime session.
pub struct LocalHostClientTransport {
    transport: Option<Arc<Mutex<Box<dyn ClientTransport>>>>,
    session: LocalHostSession,
    closed: bool,
}

impl LocalHostClientTransport {
    fn new(transport: Arc<Mutex<Box<dyn ClientTransport>>>, session: LocalHostSession) -> Self {
        Self {
            transport: Some(transport),
            session,
            closed: false,
        }
    }
}

/// Start a new local-host runtime and return a CLI-compatible transport.
pub fn connect_local_host() -> Result<LocalHostClientTransport> {
    let mut provider = LocalHostProvider::new("local-host");
    let endpoint_id = provider.create_memory_endpoint("Local Host");
    let mut session = pollster::block_on(provider.connect(&endpoint_id))?;
    let connection = pollster::block_on(session.connection())?;
    let transport = connection
        .local_host_transport()
        .ok_or_else(|| anyhow::anyhow!("local-host connection did not include a transport"))?;

    Ok(LocalHostClientTransport::new(transport, session))
}

#[async_trait::async_trait]
impl ClientTransport for LocalHostClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        let Some(transport) = &self.transport else {
            return Err(TransportError::ConnectionLost);
        };
        transport.lock().await.send(msg).await
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        let Some(transport) = &self.transport else {
            return Err(TransportError::ConnectionLost);
        };
        transport.lock().await.receive().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;
        drop(self.transport.take());
        self.session.close().await.map_err(link_error_to_transport)
    }
}

impl Drop for LocalHostClientTransport {
    fn drop(&mut self) {
        drop(self.transport.take());
    }
}

fn link_error_to_transport(error: LinkError) -> TransportError {
    TransportError::Other(error.to_string())
}

#[cfg(test)]
mod tests {
    use lpa_client::LpClient;

    use super::*;

    #[tokio::test]
    async fn local_host_transport_serves_client_requests() {
        let transport = connect_local_host().unwrap();
        let client = LpClient::new(Box::new(transport));

        let projects = client.project_list_available().await.unwrap();

        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn close_stops_local_host_transport() {
        let mut transport = connect_local_host().unwrap();

        transport.close().await.unwrap();

        assert!(matches!(
            transport.receive().await,
            Err(TransportError::ConnectionLost)
        ));
    }
}
