use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::link_provider::LinkProviderId;
use crate::link_session::LinkSessionId;
use crate::providers::host_serial_esp32::session::HostSerialEsp32Session;
use crate::{LinkCapabilities, LinkEndpoint, LinkError, LinkProvider, LinkServerConnection};
use lpa_client::transport_serial::{
    create_hardware_serial_transport_pair_with_options, HardwareSerialOptions, SerialLineObserver,
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct HostSerialEsp32Provider {
    id: LinkProviderId,
    endpoints: Vec<HostSerialEsp32Endpoint>,
    options: HostSerialEsp32Options,
    next_session_index: u64,
}

#[derive(Clone, Default)]
pub struct HostSerialEsp32Options {
    pub baud_rate: Option<u32>,
    pub reset_after_open: bool,
    pub line_observer: Option<Arc<dyn SerialLineObserver>>,
}

impl HostSerialEsp32Provider {
    pub fn new(id: impl Into<LinkProviderId>) -> Self {
        Self::with_options(id, HostSerialEsp32Options::default())
    }

    pub fn with_options(id: impl Into<LinkProviderId>, options: HostSerialEsp32Options) -> Self {
        Self {
            id: id.into(),
            endpoints: Vec::new(),
            options,
            next_session_index: 1,
        }
    }

    pub fn set_options(&mut self, options: HostSerialEsp32Options) {
        self.options = options;
    }

    pub fn options(&self) -> &HostSerialEsp32Options {
        &self.options
    }

    pub fn create_endpoint_for_port(
        &mut self,
        port_name: impl Into<String>,
        label: impl Into<String>,
    ) -> LinkEndpointId {
        let port_name = port_name.into();
        let endpoint_id = endpoint_id_for_port(&self.id, &port_name);
        self.upsert_port_endpoint(endpoint_id.clone(), port_name, label.into());
        endpoint_id
    }

    pub fn port_name_for_endpoint(&self, endpoint_id: &LinkEndpointId) -> Option<&str> {
        self.endpoints
            .iter()
            .find(|entry| entry.endpoint.id == *endpoint_id)
            .map(|entry| entry.port_name.as_str())
    }

    fn refresh_discovered_endpoints(&mut self) -> Result<(), LinkError> {
        let mut ports = serialport::available_ports()
            .map_err(|error| LinkError::other(format!("failed to list serial ports: {error}")))?
            .into_iter()
            .map(|info| info.port_name)
            .collect::<Vec<_>>();
        ports.sort();

        self.endpoints.clear();
        for port_name in ports {
            let label = label_for_port(&port_name);
            self.create_endpoint_for_port(port_name, label);
        }
        Ok(())
    }

    pub fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<&LinkEndpoint, LinkError> {
        Ok(&self.endpoint_entry(endpoint_id)?.endpoint)
    }

    fn endpoint_entry(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<&HostSerialEsp32Endpoint, LinkError> {
        self.endpoints
            .iter()
            .find(|entry| entry.endpoint.id == *endpoint_id)
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    fn upsert_port_endpoint(
        &mut self,
        endpoint_id: LinkEndpointId,
        port_name: String,
        label: String,
    ) {
        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.id.clone(), label)
            .with_capabilities(LinkCapabilities::esp32_serial_base());

        if let Some(existing) = self
            .endpoints
            .iter_mut()
            .find(|entry| entry.endpoint.id == endpoint_id)
        {
            *existing = HostSerialEsp32Endpoint {
                endpoint,
                port_name,
            };
        } else {
            self.endpoints.push(HostSerialEsp32Endpoint {
                endpoint,
                port_name,
            });
        }
    }
}

impl LinkProvider for HostSerialEsp32Provider {
    type Session = HostSerialEsp32Session;

    fn id(&self) -> &LinkProviderId {
        &self.id
    }

    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError> {
        self.refresh_discovered_endpoints()?;
        Ok(self
            .endpoints
            .iter()
            .map(|entry| entry.endpoint.clone())
            .collect())
    }

    async fn status(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status.clone())
    }

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<Self::Session, LinkError> {
        let endpoint = self.endpoint_entry(endpoint_id)?.clone();
        let session_id = LinkSessionId::new(format!(
            "{}:{}",
            endpoint_id.as_str(),
            self.next_session_index
        ));
        self.next_session_index += 1;

        let baud_rate = self
            .options
            .baud_rate
            .unwrap_or(lpc_model::DEFAULT_SERIAL_BAUD_RATE);
        let serial_options = HardwareSerialOptions {
            reset_after_open: self.options.reset_after_open,
            line_observer: self.options.line_observer.clone(),
        };
        let transport = create_hardware_serial_transport_pair_with_options(
            &endpoint.port_name,
            baud_rate,
            serial_options,
        )
        .map_err(|error| LinkError::ConnectionFailed {
            message: error.to_string(),
        })?;
        let transport: Box<dyn lpa_client::ClientTransport> = Box::new(transport);
        let server_connection: LinkServerConnection = Arc::new(Mutex::new(transport));

        Ok(HostSerialEsp32Session::new(
            endpoint.endpoint.id,
            session_id,
            endpoint.port_name,
            baud_rate,
            server_connection,
        ))
    }
}

#[derive(Clone, Debug)]
struct HostSerialEsp32Endpoint {
    endpoint: LinkEndpoint,
    port_name: String,
}

pub fn label_for_port(port_name: &str) -> String {
    if is_likely_esp32_serial_port(port_name) {
        format!("ESP32 Serial ({port_name})")
    } else {
        format!("Serial ({port_name})")
    }
}

fn endpoint_id_for_port(provider_id: &LinkProviderId, port_name: &str) -> LinkEndpointId {
    LinkEndpointId::new(format!(
        "{}:{}",
        provider_id.as_str(),
        sanitize_endpoint_part(port_name)
    ))
}

fn sanitize_endpoint_part(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub fn is_likely_esp32_serial_port(port_name: &str) -> bool {
    port_name.contains("usbmodem")
        || port_name.contains("ttyUSB")
        || port_name.contains("ttyACM")
        || port_name.contains("tty.usbserial")
}
