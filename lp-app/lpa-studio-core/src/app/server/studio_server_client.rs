use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientError, ClientEvent, ClientIo, LpClient};
use lpa_link::{LinkConnection, LinkConnectionKind};
use lpc_wire::{ProjectReadRequest, ProjectReadResponse, WireProjectHandle};

use crate::app::project::demo_project::{
    DEMO_PROJECT_ID, DEMO_PROJECT_STORAGE_ID, demo_project_deploy_files,
};
use crate::{
    LoadedProjectChoice, ProjectInventorySummary, SharedLinkRegistry, UiError, UiLogEntry,
    UiLogLevel, UxUpdateSink,
};

pub struct StudioServerClient {
    client: LpClient<Box<dyn ClientIo>>,
    protocol: String,
    pending_logs: Rc<RefCell<Vec<UiLogEntry>>>,
}

impl StudioServerClient {
    #[cfg(test)]
    pub(crate) fn from_io_for_test(protocol: impl Into<String>, io: Box<dyn ClientIo>) -> Self {
        Self {
            client: LpClient::new(io),
            protocol: protocol.into(),
            pending_logs: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn from_link_connection(
        registry: SharedLinkRegistry,
        connection: &LinkConnection,
        updates: UxUpdateSink,
    ) -> Result<Self, UiError> {
        let pending_logs = Rc::new(RefCell::new(Vec::new()));
        let protocol = connection_protocol(&connection.kind);
        let io = server_io_from_link_connection(
            registry,
            connection,
            Rc::clone(&pending_logs),
            updates,
        )?;
        Ok(Self {
            client: LpClient::new(io),
            protocol,
            pending_logs,
        })
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub async fn load_demo_project(&mut self) -> Result<LoadedDemoProject, UiError> {
        let deploy = self
            .client
            .deploy_project_files(DEMO_PROJECT_STORAGE_ID, demo_project_deploy_files())
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

    pub fn take_pending_logs(&mut self) -> Vec<UiLogEntry> {
        core::mem::take(&mut *self.pending_logs.borrow_mut())
    }
}

pub struct LoadedDemoProject {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
    pub logs: Vec<UiLogEntry>,
}

pub struct LoadedProjectCatalog {
    pub projects: Vec<LoadedProjectChoice>,
    pub logs: Vec<UiLogEntry>,
}

pub struct LoadedRunningProject {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
}

pub struct StudioProjectRead {
    pub response: ProjectReadResponse,
    pub logs: Vec<UiLogEntry>,
}

impl StudioServerClient {
    pub async fn list_loaded_projects(&mut self) -> Result<LoadedProjectCatalog, UiError> {
        let loaded = self
            .client
            .project_list_loaded()
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(loaded.events);
        logs.extend(self.take_pending_logs());
        Ok(LoadedProjectCatalog {
            projects: loaded
                .value
                .into_iter()
                .map(|project| LoadedProjectChoice::new(project.path.as_str(), project.handle.id()))
                .collect(),
            logs,
        })
    }

    pub async fn connect_loaded_project(
        &mut self,
        choice: LoadedProjectChoice,
    ) -> Result<LoadedRunningProject, UiError> {
        let inventory = self
            .client
            .project_inventory_read(WireProjectHandle::new(choice.handle_id))
            .await
            .map_err(map_client_error)?;
        self.pending_logs
            .borrow_mut()
            .extend(map_client_events(inventory.events));
        Ok(LoadedRunningProject {
            project_id: choice.project_id,
            handle_id: choice.handle_id,
            inventory: ProjectInventorySummary::from(&inventory.value),
        })
    }

    pub async fn project_read(
        &mut self,
        handle_id: u32,
        request: ProjectReadRequest,
    ) -> Result<StudioProjectRead, UiError> {
        let read = self
            .client
            .project_read(WireProjectHandle::new(handle_id), request)
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(read.events);
        logs.extend(self.take_pending_logs());
        Ok(StudioProjectRead {
            response: read.value,
            logs,
        })
    }
}

fn server_io_from_link_connection(
    _registry: SharedLinkRegistry,
    connection: &LinkConnection,
    _pending_logs: Rc<RefCell<Vec<UiLogEntry>>>,
    _updates: UxUpdateSink,
) -> Result<Box<dyn ClientIo>, UiError> {
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
        LinkConnectionKind::BrowserWorker { .. } => Err(UiError::UnsupportedFeature(
            "browser worker server I/O requires the browser-worker feature on wasm".to_string(),
        )),
        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        LinkConnectionKind::BrowserSerialEsp32 { .. } => Ok(Box::new(
            super::browser_serial_client_io::BrowserSerialClientIo::new(
                _registry,
                connection.session_id.clone(),
                _pending_logs,
                _updates,
            ),
        )),
        #[cfg(not(all(feature = "browser-serial-esp32", target_arch = "wasm32")))]
        LinkConnectionKind::BrowserSerialEsp32 { .. } => Err(UiError::UnsupportedFeature(
            "browser serial ESP32 server I/O requires the browser-serial-esp32 feature on wasm"
                .to_string(),
        )),
        LinkConnectionKind::Fake => Err(UiError::UnsupportedFeature(
            "fake links do not expose a server protocol".to_string(),
        )),
        LinkConnectionKind::HostProcess
        | LinkConnectionKind::HostSerialEsp32
        | LinkConnectionKind::PendingImplementation { .. } => Err(UiError::UnsupportedFeature(
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

fn map_client_events(events: Vec<ClientEvent>) -> Vec<UiLogEntry> {
    events
        .into_iter()
        .map(|event| match event {
            ClientEvent::Heartbeat {
                frame_count,
                uptime_ms,
                ..
            } => UiLogEntry::new(
                UiLogLevel::Debug,
                "lp-server",
                format!("heartbeat frame={frame_count} uptime_ms={uptime_ms}"),
            ),
            ClientEvent::Log { level, message } => {
                UiLogEntry::new(map_server_log_level(level), "lp-server", message)
            }
            ClientEvent::UncorrelatedResponse {
                response_id,
                expected_id,
            } => UiLogEntry::new(
                UiLogLevel::Warn,
                "lp-server",
                format!("uncorrelated response {response_id}; expected {expected_id}"),
            ),
        })
        .collect()
}

fn map_client_error(error: ClientError) -> UiError {
    match error {
        ClientError::Transport(message)
            if super::browser_serial_readiness::is_no_firmware_detected_message(&message) =>
        {
            UiError::NoFirmwareDetected(message)
        }
        ClientError::Transport(message) => UiError::Transport(message),
        ClientError::Server(message) | ClientError::Protocol(message) => UiError::Protocol(message),
        ClientError::UnexpectedResponse {
            operation,
            response,
        } => UiError::Protocol(format!("unexpected response for {operation}: {response}")),
    }
}

fn map_server_log_level(level: lpc_wire::server::api::LogLevel) -> UiLogLevel {
    match level {
        lpc_wire::server::api::LogLevel::Debug => UiLogLevel::Debug,
        lpc_wire::server::api::LogLevel::Info => UiLogLevel::Info,
        lpc_wire::server::api::LogLevel::Warn => UiLogLevel::Warn,
        lpc_wire::server::api::LogLevel::Error => UiLogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::super::browser_serial_readiness::NO_FIRMWARE_DETECTED_PREFIX;
    use super::*;

    #[test]
    fn no_firmware_transport_error_maps_to_no_firmware_ux_error() {
        let error = map_client_error(ClientError::Transport(format!(
            "Transport error: {NO_FIRMWARE_DETECTED_PREFIX}; recent serial output: invalid header"
        )));

        assert!(matches!(error, UiError::NoFirmwareDetected(_)));
    }
}
