use std::collections::BTreeMap;
use std::sync::{Arc, MutexGuard};

use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::session::LinkSessionId;
use crate::providers::{LinkProviderDescriptor, LinkProviderKind};
use crate::{
    LinkCapabilities, LinkConnection, LinkConnectionKind, LinkDiagnostic, LinkDiagnosticSeverity,
    LinkEndpoint, LinkError, LinkLogEntry, LinkLogLevel, LinkProvider, LinkServerConnection,
    LinkSession, LinkSessionStatus,
};
use lpa_client::stream::SerialPortByteStream;
use lpa_client::transport_serial::{
    HardwareSerialOptions, SerialLineObserver, create_hardware_serial_transport_pair_with_options,
};
use tokio::sync::Mutex;

pub fn descriptor() -> LinkProviderDescriptor {
    LinkProviderKind::HostSerialEsp32.descriptor()
}

/// ESP32-over-host-serial provider.
///
/// Endpoint and session state live behind an internal `Mutex` with lock
/// scopes confined to synchronous sections (never across an `await`), so the
/// provider serves `&self` callers through a shared `LinkConnector` while its
/// futures stay `Send` for host consumers such as `lp-cli`.
pub struct HostSerialEsp32Provider {
    state: std::sync::Mutex<HostSerialEsp32State>,
    options: HostSerialEsp32Options,
}

struct HostSerialEsp32State {
    endpoints: Vec<HostSerialEsp32Endpoint>,
    sessions: BTreeMap<LinkSessionId, HostSerialEsp32SessionState>,
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
            state: std::sync::Mutex::new(HostSerialEsp32State {
                endpoints: Vec::new(),
                sessions: BTreeMap::new(),
                next_session_index: 1,
            }),
            options,
        }
    }

    pub fn set_options(&mut self, options: HostSerialEsp32Options) {
        self.options = options;
    }

    pub fn options(&self) -> &HostSerialEsp32Options {
        &self.options
    }

    pub fn create_endpoint_for_port(
        &self,
        port_name: impl Into<String>,
        label: impl Into<String>,
    ) -> LinkEndpointId {
        let port_name = port_name.into();
        let endpoint_id = endpoint_id_for_port(&port_name);
        upsert_port_endpoint(
            &mut self.state(),
            self.kind(),
            endpoint_id.clone(),
            port_name,
            label.into(),
        );
        endpoint_id
    }

    pub fn port_name_for_endpoint(&self, endpoint_id: &LinkEndpointId) -> Option<String> {
        self.state()
            .endpoints
            .iter()
            .find(|entry| entry.endpoint.id == *endpoint_id)
            .map(|entry| entry.port_name.clone())
    }

    fn refresh_discovered_endpoints(&self) -> Result<(), LinkError> {
        let mut ports = serialport::available_ports()
            .map_err(|error| LinkError::other(format!("failed to list serial ports: {error}")))?
            .into_iter()
            .map(|info| info.port_name)
            .collect::<Vec<_>>();
        ports.sort();

        let mut state = self.state();
        state.endpoints.clear();
        for port_name in ports {
            let label = label_for_port(&port_name);
            let endpoint_id = endpoint_id_for_port(&port_name);
            upsert_port_endpoint(&mut state, self.kind(), endpoint_id, port_name, label);
        }
        Ok(())
    }

    pub fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpoint, LinkError> {
        Ok(self.endpoint_entry(endpoint_id)?.endpoint)
    }

    fn state(&self) -> MutexGuard<'_, HostSerialEsp32State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn endpoint_entry(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<HostSerialEsp32Endpoint, LinkError> {
        self.state()
            .endpoints
            .iter()
            .find(|entry| entry.endpoint.id == *endpoint_id)
            .cloned()
            .ok_or_else(|| LinkError::endpoint_not_found(endpoint_id.as_str()))
    }

    /// Drain the serial lines observed on a session (every complete line,
    /// protocol and logs alike) — the host analogue of the browser serial
    /// provider's `take_lines`, consumed by `DeviceSession` for boot
    /// diagnosis and the device console feed.
    pub fn take_lines(&self, session_id: &LinkSessionId) -> Result<Vec<String>, LinkError> {
        let lines = {
            let state = self.state();
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
            Arc::clone(&session.observed_lines)
        };
        let mut lines = lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(core::mem::take(&mut *lines))
    }
}

/// Buffers every observed serial line for `take_lines` while forwarding to
/// an app-supplied observer when one is configured.
struct TeeLineObserver {
    buffer: Arc<std::sync::Mutex<Vec<String>>>,
    inner: Option<Arc<dyn SerialLineObserver>>,
}

impl SerialLineObserver for TeeLineObserver {
    fn observe_line(&self, line: &str) {
        self.buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line.to_string());
        if let Some(inner) = &self.inner {
            inner.observe_line(line);
        }
    }
}

impl LinkProvider for HostSerialEsp32Provider {
    fn kind(&self) -> LinkProviderKind {
        LinkProviderKind::HostSerialEsp32
    }

    async fn discover(&self) -> Result<Vec<LinkEndpoint>, LinkError> {
        self.refresh_discovered_endpoints()?;
        Ok(self
            .state()
            .endpoints
            .iter()
            .map(|entry| entry.endpoint.clone())
            .collect())
    }

    async fn status(&self, endpoint_id: &LinkEndpointId) -> Result<LinkEndpointStatus, LinkError> {
        Ok(self.endpoint(endpoint_id)?.status)
    }

    async fn connect(&self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError> {
        let endpoint = self.endpoint_entry(endpoint_id)?;

        let baud_rate = self
            .options
            .baud_rate
            .unwrap_or(lpc_model::DEFAULT_SERIAL_BAUD_RATE);
        // Per-session buffered line tap (mirrors the fake provider's): the
        // DeviceSession drains it via `take_lines` for boot diagnosis and
        // the device console feed. An app-supplied observer still sees
        // every line too.
        let observed_lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let serial_options = HardwareSerialOptions {
            reset_after_open: self.options.reset_after_open,
            line_observer: Some(Arc::new(TeeLineObserver {
                buffer: Arc::clone(&observed_lines),
                inner: self.options.line_observer.clone(),
            })),
        };
        // Port opening happens here (the provider owns the endpoint→port
        // mapping); the transport machinery below the byte-stream seam is
        // port-agnostic and shared with the fake device.
        let stream =
            SerialPortByteStream::open(&endpoint.port_name, baud_rate).map_err(|error| {
                LinkError::ConnectionFailed {
                    message: error.to_string(),
                }
            })?;
        let transport = create_hardware_serial_transport_pair_with_options(
            Box::new(stream),
            &endpoint.port_name,
            serial_options,
        )
        .map_err(|error| LinkError::ConnectionFailed {
            message: error.to_string(),
        })?;
        let transport: Box<dyn lpa_client::ClientTransport> = Box::new(transport);
        let server_connection: LinkServerConnection = Arc::new(Mutex::new(transport));

        let mut state = self.state();
        let session_index = state.next_session_index;
        state.next_session_index += 1;
        let session_id = LinkSessionId::new(format!("{}:{}", endpoint_id.as_str(), session_index));

        let session = LinkSession::new(
            session_id.clone(),
            self.kind(),
            endpoint.endpoint.id.clone(),
            LinkConnectionKind::HostSerialEsp32,
            endpoint.endpoint.capabilities.clone(),
        );
        state.sessions.insert(
            session_id,
            HostSerialEsp32SessionState::new(
                session.clone(),
                endpoint.port_name,
                baud_rate,
                server_connection,
                observed_lines,
            ),
        );
        Ok(session)
    }

    async fn connection(&self, session_id: &LinkSessionId) -> Result<LinkConnection, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        if session.session.status == LinkSessionStatus::Closed {
            return Err(LinkError::Closed);
        }
        let Some(server_connection) = &session.server_connection else {
            return Err(LinkError::Closed);
        };
        Ok(LinkConnection::host_serial_esp32(
            session.session.endpoint_id.clone(),
            session.session.id.clone(),
            server_connection.clone(),
        ))
    }

    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(session.logs.clone())
    }

    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError> {
        let state = self.state();
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        Ok(session.diagnostics.clone())
    }

    async fn close(&self, session_id: &LinkSessionId) -> Result<(), LinkError> {
        // Mark the session closed and take the transport out of the state
        // BEFORE awaiting the transport close: no internal lock may span
        // the await.
        let server_connection = {
            let mut state = self.state();
            let session = state
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
            if session.session.status == LinkSessionStatus::Closed {
                return Ok(());
            }
            session.session.status = LinkSessionStatus::Closed;
            session.server_connection.take()
        };
        if let Some(server_connection) = server_connection {
            let mut transport = server_connection.lock().await;
            lpa_client::ClientTransport::close(&mut **transport)
                .await
                .map_err(|error| LinkError::other(error.to_string()))?;
        }
        let mut state = self.state();
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| LinkError::session_not_found(session_id.as_str()))?;
        let log = LinkLogEntry::new(
            session.session.endpoint_id.clone(),
            Some(session.session.id.clone()),
            LinkLogLevel::Info,
            format!(
                "host serial ESP32 session closed on {} at {} baud",
                session.port_name, session.baud_rate
            ),
        );
        session.logs.push(log);
        Ok(())
    }
}

fn upsert_port_endpoint(
    state: &mut HostSerialEsp32State,
    kind: LinkProviderKind,
    endpoint_id: LinkEndpointId,
    port_name: String,
    label: String,
) {
    // Only logs + diagnostics: this provider implements no `manage()`, so
    // advertising Reset would lie (`ResetRuntime` would return
    // `OperationUnsupported`). Reset/Flash/Erase return together with a
    // real management implementation (M5, espflash-lib).
    let endpoint = LinkEndpoint::new(endpoint_id.clone(), kind, label)
        .with_capabilities(LinkCapabilities::diagnostics_and_logs());

    if let Some(existing) = state
        .endpoints
        .iter_mut()
        .find(|entry| entry.endpoint.id == endpoint_id)
    {
        *existing = HostSerialEsp32Endpoint {
            endpoint,
            port_name,
        };
    } else {
        state.endpoints.push(HostSerialEsp32Endpoint {
            endpoint,
            port_name,
        });
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
    /// Buffered line tap for `take_lines` (see [`TeeLineObserver`]).
    observed_lines: Arc<std::sync::Mutex<Vec<String>>>,
    logs: Vec<LinkLogEntry>,
    diagnostics: Vec<LinkDiagnostic>,
}

impl HostSerialEsp32SessionState {
    fn new(
        session: LinkSession,
        port_name: String,
        baud_rate: u32,
        server_connection: LinkServerConnection,
        observed_lines: Arc<std::sync::Mutex<Vec<String>>>,
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
            observed_lines,
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
