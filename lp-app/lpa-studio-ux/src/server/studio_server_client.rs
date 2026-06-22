use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientError, ClientEvent, ClientIo, LpClient};
use lpa_link::{LinkConnection, LinkConnectionKind};

use crate::project::demo_project::{DEMO_PROJECT_ID, demo_project_deploy_files};
use crate::{ProjectInventorySummary, SharedLinkRegistry, UxError, UxLogEntry, UxLogLevel};

pub struct StudioServerClient {
    client: LpClient<Box<dyn ClientIo>>,
    protocol: String,
    pending_logs: Rc<RefCell<Vec<UxLogEntry>>>,
}

impl StudioServerClient {
    pub fn from_link_connection(
        registry: SharedLinkRegistry,
        connection: &LinkConnection,
    ) -> Result<Self, UxError> {
        let pending_logs = Rc::new(RefCell::new(Vec::new()));
        let protocol = connection_protocol(&connection.kind);
        let io = server_io_from_link_connection(registry, connection, Rc::clone(&pending_logs))?;
        Ok(Self {
            client: LpClient::new(io),
            protocol,
            pending_logs,
        })
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub async fn load_demo_project(&mut self) -> Result<LoadedDemoProject, UxError> {
        let deploy = self
            .client
            .deploy_project_files(DEMO_PROJECT_ID, demo_project_deploy_files())
            .await
            .map_err(map_client_error)?;
        let handle = deploy.value;
        let inventory = self
            .client
            .project_inventory_read(handle)
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(deploy.events);
        logs.extend(map_client_events(inventory.events));
        logs.extend(self.take_pending_logs());

        Ok(LoadedDemoProject {
            project_id: DEMO_PROJECT_ID.to_string(),
            handle_id: handle.id(),
            inventory: ProjectInventorySummary::from(&inventory.value),
            logs,
        })
    }

    pub fn take_pending_logs(&mut self) -> Vec<UxLogEntry> {
        core::mem::take(&mut *self.pending_logs.borrow_mut())
    }
}

pub struct LoadedDemoProject {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
    pub logs: Vec<UxLogEntry>,
}

fn server_io_from_link_connection(
    _registry: SharedLinkRegistry,
    connection: &LinkConnection,
    _pending_logs: Rc<RefCell<Vec<UxLogEntry>>>,
) -> Result<Box<dyn ClientIo>, UxError> {
    match &connection.kind {
        #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
        LinkConnectionKind::BrowserWorker { .. } => Ok(Box::new(
            super::browser_worker_client_io::BrowserWorkerClientIo::new(
                _registry,
                connection.session_id.clone(),
                _pending_logs,
            ),
        )),
        #[cfg(not(all(feature = "browser-worker", target_arch = "wasm32")))]
        LinkConnectionKind::BrowserWorker { .. } => Err(UxError::UnsupportedFeature(
            "browser worker server I/O requires the browser-worker feature on wasm".to_string(),
        )),
        LinkConnectionKind::Fake => Err(UxError::UnsupportedFeature(
            "fake links do not expose a server protocol".to_string(),
        )),
        LinkConnectionKind::HostProcess
        | LinkConnectionKind::HostSerialEsp32
        | LinkConnectionKind::BrowserSerialEsp32 { .. }
        | LinkConnectionKind::PendingImplementation { .. } => Err(UxError::UnsupportedFeature(
            format!("server I/O is not implemented for {:?}", connection.kind),
        )),
    }
}

fn connection_protocol(kind: &LinkConnectionKind) -> String {
    match kind {
        LinkConnectionKind::BrowserWorker { protocol }
        | LinkConnectionKind::BrowserSerialEsp32 { protocol } => protocol.clone(),
        LinkConnectionKind::HostProcess => "host-process".to_string(),
        LinkConnectionKind::HostSerialEsp32 => "host-serial-esp32".to_string(),
        LinkConnectionKind::Fake => "fake".to_string(),
        LinkConnectionKind::PendingImplementation { kind } => kind.clone(),
    }
}

fn map_client_events(events: Vec<ClientEvent>) -> Vec<UxLogEntry> {
    events
        .into_iter()
        .map(|event| match event {
            ClientEvent::Heartbeat {
                frame_count,
                uptime_ms,
                ..
            } => UxLogEntry::new(
                UxLogLevel::Debug,
                "lp-server",
                format!("heartbeat frame={frame_count} uptime_ms={uptime_ms}"),
            ),
            ClientEvent::Log { level, message } => {
                UxLogEntry::new(map_server_log_level(level), "lp-server", message)
            }
            ClientEvent::UncorrelatedResponse {
                response_id,
                expected_id,
            } => UxLogEntry::new(
                UxLogLevel::Warn,
                "lp-server",
                format!("uncorrelated response {response_id}; expected {expected_id}"),
            ),
        })
        .collect()
}

fn map_client_error(error: ClientError) -> UxError {
    match error {
        ClientError::Transport(message) => UxError::Transport(message),
        ClientError::Server(message) | ClientError::Protocol(message) => UxError::Protocol(message),
        ClientError::UnexpectedResponse {
            operation,
            response,
        } => UxError::Protocol(format!("unexpected response for {operation}: {response}")),
    }
}

fn map_server_log_level(level: lpc_wire::server::api::LogLevel) -> UxLogLevel {
    match level {
        lpc_wire::server::api::LogLevel::Debug => UxLogLevel::Debug,
        lpc_wire::server::api::LogLevel::Info => UxLogLevel::Info,
        lpc_wire::server::api::LogLevel::Warn => UxLogLevel::Warn,
        lpc_wire::server::api::LogLevel::Error => UxLogLevel::Error,
    }
}
