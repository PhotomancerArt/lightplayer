use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientError, ClientEvent, LpClient};
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_worker::{BrowserWorkerOptions, BrowserWorkerProvider};
use lpa_link::{LinkConnectionKind, LinkDiagnosticSeverity, LinkLogLevel, LinkProvider};

use crate::browser_worker::browser_worker_client_io::{
    BrowserWorkerClientIo, worker_output_to_log,
};
use crate::browser_worker::demo_project::{DEMO_PROJECT_ID, demo_project_deploy_files};
use crate::{ConnectedDeviceSummary, ProjectInventorySummary, UxError, UxLogEntry, UxLogLevel};

pub struct BrowserWorkerRuntime {
    provider: Rc<RefCell<BrowserWorkerProvider>>,
    session_id: Option<LinkSessionId>,
    client: Option<LpClient<BrowserWorkerClientIo>>,
    logs: Rc<RefCell<Vec<UxLogEntry>>>,
}

impl BrowserWorkerRuntime {
    pub fn new() -> Self {
        Self::with_options(BrowserWorkerOptions::default())
    }

    pub fn with_options(options: BrowserWorkerOptions) -> Self {
        let mut provider = BrowserWorkerProvider::with_options(options);
        provider.create_worker_endpoint("Browser firmware runtime");
        Self {
            provider: Rc::new(RefCell::new(provider)),
            session_id: None,
            client: None,
            logs: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub async fn start(&mut self) -> Result<BrowserWorkerStarted, UxError> {
        let endpoint = self
            .provider
            .borrow_mut()
            .discover()
            .await
            .map_err(map_link_error)?
            .into_iter()
            .next()
            .ok_or_else(|| UxError::Link("browser worker provider has no endpoint".into()))?;
        let session = self
            .provider
            .borrow_mut()
            .connect(&endpoint.id)
            .await
            .map_err(map_link_error)?;
        let connection = self
            .provider
            .borrow_mut()
            .connection(session.id())
            .await
            .map_err(map_link_error)?;
        let protocol = match connection.kind {
            LinkConnectionKind::BrowserWorker { protocol } => protocol,
            other => {
                return Err(UxError::Link(format!(
                    "browser worker returned unexpected connection kind: {other:?}"
                )));
            }
        };
        let session_id = session.id().clone();
        let mut logs = self.provider_logs(&session_id)?;
        logs.extend(self.take_worker_logs(&session_id)?);
        self.client = Some(LpClient::new(BrowserWorkerClientIo::new(
            Rc::clone(&self.provider),
            session_id.clone(),
            Rc::clone(&self.logs),
        )));
        self.session_id = Some(session_id.clone());

        Ok(BrowserWorkerStarted {
            device: ConnectedDeviceSummary::new(
                endpoint.provider_kind,
                endpoint.id.as_str(),
                session_id.as_str(),
                endpoint.label,
            ),
            protocol,
            logs,
        })
    }

    pub async fn load_demo_project(&mut self) -> Result<BrowserWorkerProjectLoaded, UxError> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| UxError::MissingSession("browser worker client is missing".into()))?;
        let deploy = client
            .deploy_project_files(DEMO_PROJECT_ID, demo_project_deploy_files())
            .await
            .map_err(map_client_error)?;
        let handle = deploy.value;
        let inventory = client
            .project_inventory_read(handle)
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(deploy.events);
        logs.extend(map_client_events(inventory.events));
        logs.extend(self.take_pending_logs());

        Ok(BrowserWorkerProjectLoaded {
            project_id: DEMO_PROJECT_ID.to_string(),
            handle_id: handle.id(),
            inventory: ProjectInventorySummary::from(&inventory.value),
            logs,
        })
    }

    fn provider_logs(&self, session_id: &LinkSessionId) -> Result<Vec<UxLogEntry>, UxError> {
        let provider = self.provider.borrow();
        let mut logs = provider
            .logs(session_id)
            .map_err(map_link_error)?
            .into_iter()
            .map(|entry| {
                UxLogEntry::new(map_link_log_level(entry.level), "lpa-link", entry.message)
            })
            .collect::<Vec<_>>();
        logs.extend(
            provider
                .diagnostics(session_id)
                .map_err(map_link_error)?
                .into_iter()
                .map(|diagnostic| {
                    UxLogEntry::new(
                        map_diagnostic_level(diagnostic.severity),
                        "lpa-link",
                        diagnostic.message,
                    )
                }),
        );
        Ok(logs)
    }

    fn take_worker_logs(&self, session_id: &LinkSessionId) -> Result<Vec<UxLogEntry>, UxError> {
        Ok(self
            .provider
            .borrow_mut()
            .take_outputs(session_id)
            .map_err(map_link_error)?
            .into_iter()
            .filter_map(worker_output_to_log)
            .collect())
    }

    fn take_pending_logs(&self) -> Vec<UxLogEntry> {
        core::mem::take(&mut *self.logs.borrow_mut())
    }
}

pub struct BrowserWorkerStarted {
    pub device: ConnectedDeviceSummary,
    pub protocol: String,
    pub logs: Vec<UxLogEntry>,
}

pub struct BrowserWorkerProjectLoaded {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
    pub logs: Vec<UxLogEntry>,
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

fn map_link_error(error: impl core::fmt::Display) -> UxError {
    UxError::Link(error.to_string())
}

fn map_link_log_level(level: LinkLogLevel) -> UxLogLevel {
    match level {
        LinkLogLevel::Trace | LinkLogLevel::Debug => UxLogLevel::Debug,
        LinkLogLevel::Info => UxLogLevel::Info,
        LinkLogLevel::Warn => UxLogLevel::Warn,
        LinkLogLevel::Error => UxLogLevel::Error,
    }
}

fn map_diagnostic_level(level: LinkDiagnosticSeverity) -> UxLogLevel {
    match level {
        LinkDiagnosticSeverity::Info => UxLogLevel::Info,
        LinkDiagnosticSeverity::Warning => UxLogLevel::Warn,
        LinkDiagnosticSeverity::Error => UxLogLevel::Error,
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
