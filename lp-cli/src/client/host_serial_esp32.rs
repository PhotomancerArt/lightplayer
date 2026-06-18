//! CLI host-serial-ESP32 transport.
//!
//! This module adapts `lpa-link`'s `host-serial-esp32` provider to the CLI's
//! current `ClientTransport` return shape while keeping the link session alive
//! for the lifetime of the transport.

use anyhow::Result;
use lpa_client::ClientTransport;
use lpa_link::providers::host_serial_esp32::{
    HostSerialEsp32Options, HostSerialEsp32Provider, HostSerialEsp32Session,
};
use lpa_link::{LinkError, LinkProvider, LinkSession};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Client transport backed by an ESP32 over host serial.
pub struct HostSerialEsp32ClientTransport {
    transport: Option<Arc<Mutex<Box<dyn ClientTransport>>>>,
    session: HostSerialEsp32Session,
    closed: bool,
}

impl HostSerialEsp32ClientTransport {
    fn new(
        transport: Arc<Mutex<Box<dyn ClientTransport>>>,
        session: HostSerialEsp32Session,
    ) -> Self {
        Self {
            transport: Some(transport),
            session,
            closed: false,
        }
    }
}

pub fn connect_host_serial_esp32(
    port_name: &str,
    baud_rate: u32,
) -> Result<HostSerialEsp32ClientTransport> {
    connect_host_serial_esp32_with_options(
        port_name,
        HostSerialEsp32Options {
            baud_rate: Some(baud_rate),
            ..HostSerialEsp32Options::default()
        },
    )
}

pub fn connect_host_serial_esp32_with_options(
    port_name: &str,
    options: HostSerialEsp32Options,
) -> Result<HostSerialEsp32ClientTransport> {
    let mut provider = HostSerialEsp32Provider::with_options("host-serial-esp32", options);
    let endpoint_id = provider.create_endpoint_for_port(port_name, format!("ESP32 ({port_name})"));
    let mut session = pollster::block_on(provider.connect(&endpoint_id))?;
    let connection = pollster::block_on(session.connection())?;
    let transport = connection.server_connection().ok_or_else(|| {
        anyhow::anyhow!("host-serial-esp32 connection did not include a transport")
    })?;

    Ok(HostSerialEsp32ClientTransport::new(transport, session))
}

#[async_trait::async_trait]
impl ClientTransport for HostSerialEsp32ClientTransport {
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

impl Drop for HostSerialEsp32ClientTransport {
    fn drop(&mut self) {
        drop(self.transport.take());
    }
}

fn link_error_to_transport(error: LinkError) -> TransportError {
    TransportError::Other(error.to_string())
}
