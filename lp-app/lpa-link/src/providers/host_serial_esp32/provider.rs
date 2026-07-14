use std::collections::BTreeMap;
use std::sync::Arc;

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkProvider, LinkServerConnection,
    LinkSession, LinkSessionStatus,
};
use lpa_client::transport_serial::{
    HardwareSerialOptions, SerialLineObserver, create_hardware_serial_transport_pair_with_options,
};
use tokio::sync::Mutex;

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::HostSerialEsp32.descriptor()
}

pub struct HostSerialEsp32Provider {
    endpoints: Vec<HostSerialEsp32Endpoint>,
    sessions: BTreeMap<LinkSessionId, HostSerialEsp32SessionState>,
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
    pub fn new() -> Self {
        Self::with_options(HostSerialEsp32Options::default())
    }

    pub fn with_options(options: HostSerialEsp32Options) -> Self {
        Self {
            endpoints: Vec::new(),
            sessions: BTreeMap::new(),
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
        let endpoint_id = endpoint_id_for_port(&port_name);
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

    fn session(
        &self,
        session_id: &LinkSessionId,
    ) -> Result<&HostSerialEsp32SessionState, LinkError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn session_mut(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<&mut HostSerialEsp32SessionState, LinkError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))
    }

    fn upsert_port_endpoint(
        &mut self,
        endpoint_id: LinkEndpointId,
        port_name: String,
        label: String,
    ) {
        // Only logs + diagnostics: this provider implements no `manage()`, so
        // advertising Reset would lie (`ResetRuntime` would return
        // `OperationUnsupported`). Reset/Flash/Erase return together with a
        // real management implementation (M5, espflash-lib).
        let endpoint = LinkEndpoint::new(endpoint_id.clone(), self.kind(), label)
            .with_capabilities(LinkCapabilities::diagnostics_and_logs());

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
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::HostSerialEsp32
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

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
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

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.endpoint.id.clone(),
            LinkConnectionKind::HostSerialEsp32,
            endpoint.endpoint.capabilities.clone(),
        );
        self.sessions.insert(
            session_id,
            HostSerialEsp32SessionState::new(
                session.clone(),
                endpoint.port_name,
                baud_rate,
                server_connection,
            ),
        );
        Ok(session)
    }

    async fn connection(
        &mut self,
        session_id: &LinkSessionId,
    ) -> Result<LinkConnection, LinkError> {
        let state = self.session(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        let Some(server_connection) = &state.server_connection else {
            return Err(LinkError::Closed);
        };
        Ok(LinkConnection::host_serial_esp32(
            state.session.endpoint_id.clone(),
            state.session.id.clone(),
            server_connection.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        Ok(self.session(session_id)?.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        Ok(self.session(session_id)?.diagnostics.clone())
    }

    async fn close(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        let state = self.session_mut(session_id)?;
        if state.session.status == LinkSessionStatus::Closed {
            return Ok(());
        }
        state.session.status = LinkSessionStatus::Closed;
        if let Some(server_connection) = state.server_connection.take() {
            let mut transport = server_connection.lock().await;
            lpa_client::ClientTransport::close(&mut **transport)
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        state.logs.push(LinkLogEntry::new(
            state.session.endpoint_id.clone(),
            Some(state.session.id.clone()),
            LinkLogLevel::Info,
            format!(
                "host serial ESP32 session closed on {} at {} baud",
                state.port_name, state.baud_rate
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

struct HostSerialEsp32SessionState {
    session: LinkSession,
    port_name: String,
    baud_rate: u32,
    server_connection: Option<LinkServerConnection>,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl HostSerialEsp32SessionState {
    fn new(
        session: LinkSession,
        port_name: String,
        baud_rate: u32,
        server_connection: LinkServerConnection,
    ) -> Self {
        let logs = vec![LinkLogEntry::new(
            session.endpoint_id.clone(),
            Some(session.id.clone()),
            LinkLogLevel::Info,
            format!("host serial ESP32 session opened on {port_name}"),
        )];
        let diagnostics = vec![LinkDiagnostic::new(
            session.endpoint_id.clone(),
            Some(session.id.clone()),
            LinkDiagnosticSeverity::Info,
            format!("host serial ESP32 transport ready at {baud_rate} baud"),
        )];
        Self {
            session,
            port_name,
            baud_rate,
            server_connection: Some(server_connection),
            logs,
            diagnostics,
        }
    }
}

pub fn label_for_port(port_name: &str) -> String {
    if is_likely_esp32_serial_port(port_name) {
        format!("ESP32 Serial ({port_name})")
    } else {
        format!("Serial ({port_name})")
    }
}

fn endpoint_id_for_port(port_name: &str) -> LinkEndpointId {
    LinkEndpointId::new(format!(
        "{}:{}",
        LinkProviderKind::HostSerialEsp32.key(),
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

/// Prefer macOS call-out (`/dev/cu.*`) devices over their `/dev/tty.*` twins.
///
/// macOS exposes each serial device as both `/dev/tty.*` (dial-in, blocks on
/// carrier detect) and `/dev/cu.*` (call-out, opens immediately). ESP32 boards
/// never assert DCD, so the `cu.*` twin is the one that works. When any
/// `/dev/cu.*` candidates exist, only those are returned; otherwise (e.g. on
/// Linux) the input is returned unchanged.
pub fn prefer_cu_ports(ports: Vec<String>) -> Vec<String> {
    let cu_ports: Vec<String> = ports
        .iter()
        .filter(|name| name.starts_with("/dev/cu."))
        .cloned()
        .collect();
    if cu_ports.is_empty() { ports } else { cu_ports }
}
