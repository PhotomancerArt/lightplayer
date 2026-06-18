use std::sync::Arc;

use lpa_client::transport_serial::{
    HardwareSerialOptions, SerialLineObserver, create_hardware_serial_transport_pair_with_options,
};
use tokio::sync::Mutex;

use crate::{
    LinkClientTransport, LinkConnection, LinkDiagnostic, LinkDiagnosticSeverity, LinkEndpoint,
    LinkEndpointId, LinkEndpointStatus, LinkError, LinkLogEntry, LinkLogLevel, LinkManagement,
    LinkProvider, LinkProviderId, LinkSession, LinkSessionId,
};

#[derive(Clone, Default)]
pub struct HostSerialEsp32Options {
    pub baud_rate: Option<u32>,
    pub reset_after_open: bool,
    pub line_observer: Option<Arc<dyn SerialLineObserver>>,
}

#[derive(Clone)]
pub struct HostSerialEsp32Provider {
    id: LinkProviderId,
    endpoints: Vec<HostSerialEsp32Endpoint>,
    options: HostSerialEsp32Options,
    next_session_index: u64,
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

    fn endpoint(
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
            .with_management(LinkManagement::esp32_serial_base());

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
        Ok(self.endpoint(endpoint_id)?.endpoint.status.clone())
    }

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<Self::Session, LinkError> {
        let endpoint = self.endpoint(endpoint_id)?.clone();
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
        let transport: LinkClientTransport = Arc::new(Mutex::new(transport));

        Ok(HostSerialEsp32Session::new(
            endpoint.endpoint.id,
            session_id,
            endpoint.port_name,
            baud_rate,
            transport,
        ))
    }
}

pub struct HostSerialEsp32Session {
    endpoint_id: LinkEndpointId,
    id: LinkSessionId,
    port_name: String,
    baud_rate: u32,
    transport: Option<LinkClientTransport>,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
    closed: bool,
}

impl HostSerialEsp32Session {
    pub fn new(
        endpoint_id: LinkEndpointId,
        id: LinkSessionId,
        port_name: String,
        baud_rate: u32,
        transport: LinkClientTransport,
    ) -> Self {
        let logs = vec![LinkLogEntry::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkLogLevel::Info,
            format!("host serial ESP32 session opened on {port_name}"),
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            endpoint_id.clone(),
            Some(id.clone()),
            LinkDiagnosticSeverity::Info,
            format!("host serial ESP32 transport ready at {baud_rate} baud"),
        )];
        Self {
            endpoint_id,
            id,
            port_name,
            baud_rate,
            transport: Some(transport),
            logs,
            diagnostics,
            closed: false,
        }
    }
}

impl LinkSession for HostSerialEsp32Session {
    fn id(&self) -> &LinkSessionId {
        &self.id
    }

    fn endpoint_id(&self) -> &LinkEndpointId {
        &self.endpoint_id
    }

    fn logs(&self) -> Vec<LinkLogEntry> {
        self.logs.clone()
    }

    fn diagnostics(&self) -> Vec<LinkDiagnostic> {
        self.diagnostics.clone()
    }

    async fn connection(&mut self) -> Result<LinkConnection, LinkError> {
        if self.closed {
            return Err(LinkError::Closed);
        }
        let Some(transport) = &self.transport else {
            return Err(LinkError::Closed);
        };
        Ok(LinkConnection::host_serial_esp32(
            self.endpoint_id.clone(),
            self.id.clone(),
            transport.clone(),
        ))
    }

    async fn close(&mut self) -> Result<(), LinkError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        if let Some(transport) = self.transport.take() {
            transport
                .lock()
                .await
                .close()
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        self.logs.push(LinkLogEntry::new(
            self.endpoint_id.clone(),
            Some(self.id.clone()),
            LinkLogLevel::Info,
            format!(
                "host serial ESP32 session closed on {} at {} baud",
                self.port_name, self.baud_rate
            ),
        ));
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct HostSerialEsp32Endpoint {
    endpoint: LinkEndpoint,
    port_name: String,
}

fn label_for_port(port_name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

    use super::*;

    #[test]
    fn explicit_port_endpoint_records_metadata() {
        let mut provider = HostSerialEsp32Provider::new("host-serial-esp32");

        let endpoint_id = provider.create_endpoint_for_port("/dev/cu.usbmodem2101", "Board");

        assert_eq!(
            endpoint_id.as_str(),
            "host-serial-esp32:dev-cu-usbmodem2101"
        );
        assert_eq!(
            provider.port_name_for_endpoint(&endpoint_id),
            Some("/dev/cu.usbmodem2101")
        );
        let endpoint = provider.endpoint(&endpoint_id).unwrap();
        assert!(endpoint.endpoint.management.can_reset);
        assert!(endpoint.endpoint.management.can_read_logs);
        assert!(endpoint.endpoint.management.can_read_diagnostics);
        assert!(!endpoint.endpoint.management.can_flash);
    }

    #[test]
    fn labels_likely_esp32_ports() {
        assert_eq!(
            label_for_port("/dev/cu.usbmodem2101"),
            "ESP32 Serial (/dev/cu.usbmodem2101)"
        );
        assert_eq!(
            label_for_port("/dev/cu.Bluetooth"),
            "Serial (/dev/cu.Bluetooth)"
        );
    }

    #[test]
    fn default_options_do_not_reset_after_open() {
        let provider = HostSerialEsp32Provider::new("host-serial-esp32");

        assert_eq!(provider.options.baud_rate, None);
        assert!(!provider.options.reset_after_open);
        assert_eq!(
            provider
                .options
                .baud_rate
                .unwrap_or(DEFAULT_SERIAL_BAUD_RATE),
            DEFAULT_SERIAL_BAUD_RATE
        );
    }
}
