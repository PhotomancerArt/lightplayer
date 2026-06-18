//! CLI host-process transport.
//!
//! This module adapts `lpa-link`'s `host-process` provider to the CLI's current
//! `ClientTransport` return shape while keeping the link session alive for the
//! lifetime of the transport.

use anyhow::Result;
use lpa_client::ClientTransport;
use lpa_link::providers::host_process::{HostProcessProvider, HostProcessSession};
use lpa_link::{LinkError, LinkProvider, LinkSession};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Client transport backed by an in-process `fw-host` runtime session.
pub struct HostProcessClientTransport {
    transport: Option<Arc<Mutex<Box<dyn ClientTransport>>>>,
    session: HostProcessSession,
    closed: bool,
}

impl HostProcessClientTransport {
    fn new(transport: Arc<Mutex<Box<dyn ClientTransport>>>, session: HostProcessSession) -> Self {
        Self {
            transport: Some(transport),
            session,
            closed: false,
        }
    }
}

/// Start a new host-process runtime and return a CLI-compatible transport.
pub fn connect_host_process() -> Result<HostProcessClientTransport> {
    let mut provider = HostProcessProvider::new("host-process");
    let endpoint_id = provider.create_memory_endpoint("Host Process");
    let mut session = pollster::block_on(provider.connect(&endpoint_id))?;
    let connection = pollster::block_on(session.connection())?;
    let transport = connection
        .client_transport()
        .ok_or_else(|| anyhow::anyhow!("host-process connection did not include a transport"))?;

    Ok(HostProcessClientTransport::new(transport, session))
}

#[async_trait::async_trait]
impl ClientTransport for HostProcessClientTransport {
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

impl Drop for HostProcessClientTransport {
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
    async fn host_process_transport_serves_client_requests() {
        let transport = connect_host_process().unwrap();
        let client = LpClient::new(Box::new(transport));

        let projects = client.project_list_available().await.unwrap();

        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn close_stops_host_process_transport() {
        let mut transport = connect_host_process().unwrap();

        transport.close().await.unwrap();

        assert!(matches!(
            transport.receive().await,
            Err(TransportError::ConnectionLost)
        ));
    }
}
