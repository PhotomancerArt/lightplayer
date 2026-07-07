use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use std::collections::BTreeMap;

use lpa_client::{
    CancelSignal, ClientError, ClientEvent, ClientIo, LpClient, ProgressDeadline, PullOutcome,
};
use lpa_link::{LinkConnection, LinkConnectionKind};
use lpc_model::{
    ArtifactLocation, CommitResult, MutationCmdBatch, MutationCmdBatchResult, NodeId,
    ProjectOverlay, Revision,
};
use lpc_wire::{
    ProjectReadEvent, ProjectReadRequest, WireOverlayMutationRequest, WireProjectHandle,
    WireProjectInventoryReadResponse,
};

use crate::app::project::demo_project::{
    DEMO_PROJECT_ID, DEMO_PROJECT_STORAGE_ID, demo_project_deploy_files,
};
use crate::{
    LoadedProjectChoice, ProjectInventorySummary, SharedLinkRegistry, UiError, UiLogDraft,
    UiLogLevel, UiLogOrigin, UxUpdateSink,
};

pub struct StudioServerClient {
    client: LpClient<Box<dyn ClientIo>>,
    protocol: String,
    pending_logs: Rc<RefCell<Vec<UiLogDraft>>>,
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
            node_def_artifacts: node_def_artifacts(&inventory.value),
            logs,
        })
    }

    pub fn take_pending_logs(&mut self) -> Vec<UiLogDraft> {
        core::mem::take(&mut *self.pending_logs.borrow_mut())
    }
}

pub struct LoadedDemoProject {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
    /// Runtime node id → containing def artifact, from the connect-time
    /// inventory read. Wire mutations target `(ArtifactLocation, SlotPath)`,
    /// so slot edits resolve their artifact through this map.
    pub node_def_artifacts: BTreeMap<NodeId, ArtifactLocation>,
    pub logs: Vec<UiLogDraft>,
}

pub struct LoadedProjectCatalog {
    pub projects: Vec<LoadedProjectChoice>,
    pub logs: Vec<UiLogDraft>,
}

pub struct LoadedRunningProject {
    pub project_id: String,
    pub handle_id: u32,
    pub inventory: ProjectInventorySummary,
    /// Runtime node id → containing def artifact (see
    /// [`LoadedDemoProject::node_def_artifacts`]).
    pub node_def_artifacts: BTreeMap<NodeId, ArtifactLocation>,
}

pub struct StudioProjectRead {
    pub events: Vec<ProjectReadEvent>,
    pub logs: Vec<UiLogDraft>,
}

/// Full pending-edit overlay pulled from the server, with the revision at
/// which it last changed (for stamping the client mirror).
pub struct StudioOverlayRead {
    pub overlay: ProjectOverlay,
    pub revision: Revision,
    pub logs: Vec<UiLogDraft>,
}

/// Per-command results of an overlay mutation batch, with the post-mutation
/// overlay revision (for `ProjectSync::apply_acked_edits`).
pub struct StudioOverlayMutation {
    pub result: MutationCmdBatchResult,
    pub overlay_revision: Revision,
    pub logs: Vec<UiLogDraft>,
}

/// Result of an overlay commit, with the post-commit overlay revision.
pub struct StudioOverlayCommit {
    pub result: CommitResult,
    pub overlay_revision: Revision,
    pub logs: Vec<UiLogDraft>,
}

impl StudioServerClient {
    /// Ask the connected server to apply `level` as its process-global log
    /// level (via the wire `SetLogLevel` command). Not persisted device-side;
    /// a reboot reverts to the logger-init default. Returns the log drafts
    /// carried by the exchange, which the caller records like any other op.
    pub async fn set_log_level(&mut self, level: UiLogLevel) -> Result<Vec<UiLogDraft>, UiError> {
        let outcome = self
            .client
            .set_log_level(wire_log_level(level))
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(outcome.events);
        logs.extend(self.take_pending_logs());
        Ok(logs)
    }

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
            node_def_artifacts: node_def_artifacts(&inventory.value),
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
            events: read.value,
            logs,
        })
    }

    /// Read the full pending-edit overlay (a sequential command on the same
    /// connection, issued after a streamed project read completes).
    pub async fn project_overlay_read(
        &mut self,
        handle_id: u32,
    ) -> Result<StudioOverlayRead, UiError> {
        let read = self
            .client
            .project_overlay_read(WireProjectHandle::new(handle_id))
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(read.events);
        logs.extend(self.take_pending_logs());
        Ok(StudioOverlayRead {
            overlay: read.value.overlay,
            revision: read.value.revision,
            logs,
        })
    }

    /// Send an ordered overlay mutation batch and collect the per-command
    /// results (accepted/rejected by `MutationCmdId`).
    pub async fn project_overlay_mutate(
        &mut self,
        handle_id: u32,
        batch: MutationCmdBatch,
    ) -> Result<StudioOverlayMutation, UiError> {
        let response = self
            .client
            .project_overlay_mutate(
                WireProjectHandle::new(handle_id),
                WireOverlayMutationRequest::new(batch),
            )
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(response.events);
        logs.extend(self.take_pending_logs());
        Ok(StudioOverlayMutation {
            result: response.value.result,
            overlay_revision: response.value.overlay_revision,
            logs,
        })
    }

    /// Commit the pending-edit overlay to artifact storage. Post-P2, transient
    /// entries survive the commit as pending overlay edits.
    pub async fn project_overlay_commit(
        &mut self,
        handle_id: u32,
    ) -> Result<StudioOverlayCommit, UiError> {
        let response = self
            .client
            .project_overlay_commit(WireProjectHandle::new(handle_id))
            .await
            .map_err(map_client_error)?;
        let mut logs = map_client_events(response.events);
        logs.extend(self.take_pending_logs());
        Ok(StudioOverlayCommit {
            result: response.value.result,
            overlay_revision: response.value.overlay_revision,
            logs,
        })
    }

    /// A project read driven under a progress deadline and cancel signal.
    ///
    /// The studio actor uses this for passive refresh ticks so it can cancel an
    /// in-flight pull cleanly (a preempting command flips `cancel`) and bound a
    /// stalled stream by the class's quiet-gap deadline. Cancel/timeout are
    /// surfaced as [`StudioProjectReadOutcome`] variants rather than errors so
    /// the caller can treat them as ordinary, non-failing control flow.
    pub async fn project_read_gated<MakeTimer, Timer, Cancel>(
        &mut self,
        handle_id: u32,
        request: ProjectReadRequest,
        deadline: ProgressDeadline<MakeTimer, Timer>,
        cancel: &Cancel,
    ) -> Result<StudioProjectReadOutcome, UiError>
    where
        MakeTimer: FnMut(Duration) -> Timer,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        match self
            .client
            .project_read_gated(WireProjectHandle::new(handle_id), request, deadline, cancel)
            .await
        {
            PullOutcome::Completed { events, observed } => {
                let mut logs = map_client_events(observed);
                logs.extend(self.take_pending_logs());
                Ok(StudioProjectReadOutcome::Completed(StudioProjectRead {
                    events,
                    logs,
                }))
            }
            PullOutcome::Cancelled => Ok(StudioProjectReadOutcome::Cancelled),
            PullOutcome::TimedOut => Ok(StudioProjectReadOutcome::TimedOut),
            PullOutcome::Failed(error) => Err(map_client_error(error)),
        }
    }
}

/// Outcome of a [`StudioServerClient::project_read_gated`] pull.
pub enum StudioProjectReadOutcome {
    /// The stream reached `fin`; the read events and logs are collected.
    Completed(StudioProjectRead),
    /// The caller's cancel signal was observed at a frame boundary.
    Cancelled,
    /// The progress deadline fired: no frame arrived within the quiet-gap budget.
    TimedOut,
}

/// Build the runtime-node-id → def-artifact map from an inventory read.
///
/// Only node uses the runtime currently instantiates carry a `runtime_id`;
/// those are exactly the nodes slot edits can address.
fn node_def_artifacts(
    inventory: &WireProjectInventoryReadResponse,
) -> BTreeMap<NodeId, ArtifactLocation> {
    inventory
        .nodes
        .iter()
        .filter_map(|node| {
            node.runtime_id
                .map(|id| (id, node.def_location.artifact.clone()))
        })
        .collect()
}

fn server_io_from_link_connection(
    _registry: SharedLinkRegistry,
    connection: &LinkConnection,
    _pending_logs: Rc<RefCell<Vec<UiLogDraft>>>,
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

/// Map side-channel client events to console log drafts.
///
/// Healthy heartbeats are telemetry, not log content: they arrive every
/// second and were the dominant console noise, so they produce no entry at
/// all (P3 ingestion hygiene; the user decision was full removal, not a
/// Trace demotion). Heartbeats reporting a recovery condition — safe mode or
/// a non-green recovery level — still surface as Warn/Error entries, and
/// server log lines pass through unchanged.
///
/// Response-correlation events split by intent: a stale drop (late response
/// for a request the client itself abandoned — routine when edit-op
/// preemption cancels a pull mid-stream during an input flood) is a
/// debug-level note, while a genuinely uncorrelated id (never issued or
/// abandoned by this session: from the future, or a duplicate delivery)
/// remains a warning.
fn map_client_events(events: Vec<ClientEvent>) -> Vec<UiLogDraft> {
    events
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Heartbeat { recovery, .. } => match recovery {
                Some(recovery)
                    if recovery.safe_mode
                        || recovery.level != lpc_wire::server::RecoveryLevelWire::Green =>
                {
                    let ui_level = match recovery.level {
                        lpc_wire::server::RecoveryLevelWire::Red => UiLogLevel::Error,
                        _ => UiLogLevel::Warn,
                    };
                    let mut message = format!(
                        "recovery level {:?}{}",
                        recovery.level,
                        if recovery.safe_mode {
                            " (SAFE MODE)"
                        } else {
                            ""
                        }
                    );
                    if let Some(crash) = &recovery.last_crash {
                        message
                            .push_str(&format!("; last crash: {} at {}", crash.cause, crash.path));
                    }
                    Some(UiLogDraft::new(ui_level, UiLogOrigin::Server, message))
                }
                _ => None,
            },
            ClientEvent::Log { level, message } => Some(UiLogDraft::new(
                map_server_log_level(level),
                UiLogOrigin::Server,
                message,
            )),
            ClientEvent::UncorrelatedResponse {
                response_id,
                expected_id,
            } => Some(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Server,
                format!("uncorrelated response {response_id}; expected {expected_id}"),
            )),
            ClientEvent::StaleResponseDropped { response_id } => Some(UiLogDraft::new(
                UiLogLevel::Debug,
                UiLogOrigin::Server,
                format!("dropped stale response {response_id} for a request abandoned by client"),
            )),
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
        lpc_wire::server::api::LogLevel::Trace => UiLogLevel::Trace,
        lpc_wire::server::api::LogLevel::Debug => UiLogLevel::Debug,
        lpc_wire::server::api::LogLevel::Info => UiLogLevel::Info,
        lpc_wire::server::api::LogLevel::Warn => UiLogLevel::Warn,
        lpc_wire::server::api::LogLevel::Error => UiLogLevel::Error,
    }
}

/// Inverse of [`map_server_log_level`], for the `SetLogLevel` request. Total:
/// both enums carry exactly Trace..Error (the wire deliberately has no `Off`).
fn wire_log_level(level: UiLogLevel) -> lpc_wire::server::api::LogLevel {
    match level {
        UiLogLevel::Trace => lpc_wire::server::api::LogLevel::Trace,
        UiLogLevel::Debug => lpc_wire::server::api::LogLevel::Debug,
        UiLogLevel::Info => lpc_wire::server::api::LogLevel::Info,
        UiLogLevel::Warn => lpc_wire::server::api::LogLevel::Warn,
        UiLogLevel::Error => lpc_wire::server::api::LogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use lpc_wire::server::{CrashSummaryWire, RecoveryLevelWire, RecoveryStatus, SampleStats};

    use super::super::browser_serial_readiness::NO_FIRMWARE_DETECTED_PREFIX;
    use super::*;

    #[test]
    fn no_firmware_transport_error_maps_to_no_firmware_ux_error() {
        let error = map_client_error(ClientError::Transport(format!(
            "Transport error: {NO_FIRMWARE_DETECTED_PREFIX}; recent serial output: invalid header"
        )));

        assert!(matches!(error, UiError::NoFirmwareDetected(_)));
    }

    #[test]
    fn healthy_heartbeats_produce_no_log_entries() {
        let events = vec![
            heartbeat_event(None),
            heartbeat_event(Some(recovery_status(RecoveryLevelWire::Green, false, None))),
        ];

        assert!(map_client_events(events).is_empty());
    }

    #[test]
    fn red_recovery_heartbeat_still_logs_an_error() {
        let crash = CrashSummaryWire {
            cause: "panic".to_string(),
            path: "node:nodes/fire".to_string(),
            message: "boom".to_string(),
            boots_ago: 0,
        };
        let events = vec![heartbeat_event(Some(recovery_status(
            RecoveryLevelWire::Red,
            false,
            Some(crash),
        )))];

        let logs = map_client_events(events);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, UiLogLevel::Error);
        assert_eq!(logs[0].source, UiLogOrigin::Server.into());
        assert!(logs[0].message.contains("recovery level Red"));
        assert!(
            logs[0]
                .message
                .contains("last crash: panic at node:nodes/fire")
        );
    }

    #[test]
    fn safe_mode_heartbeat_still_logs_a_warning() {
        let events = vec![heartbeat_event(Some(recovery_status(
            RecoveryLevelWire::Green,
            true,
            None,
        )))];

        let logs = map_client_events(events);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, UiLogLevel::Warn);
        assert!(logs[0].message.contains("(SAFE MODE)"));
    }

    #[test]
    fn yellow_recovery_heartbeat_still_logs_a_warning() {
        let events = vec![heartbeat_event(Some(recovery_status(
            RecoveryLevelWire::Yellow,
            false,
            None,
        )))];

        let logs = map_client_events(events);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, UiLogLevel::Warn);
        assert!(logs[0].message.contains("recovery level Yellow"));
    }

    #[test]
    fn server_logs_and_uncorrelated_responses_still_map() {
        // A genuinely unknown response id (never issued or abandoned by the
        // session) is a real protocol anomaly and keeps its warning.
        let events = vec![
            ClientEvent::Log {
                level: lpc_wire::server::api::LogLevel::Warn,
                message: "flash almost full".to_string(),
            },
            ClientEvent::UncorrelatedResponse {
                response_id: 9,
                expected_id: 7,
            },
        ];

        let logs = map_client_events(events);

        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, UiLogLevel::Warn);
        assert_eq!(logs[0].message, "flash almost full");
        assert_eq!(logs[1].level, UiLogLevel::Warn);
        assert_eq!(logs[1].message, "uncorrelated response 9; expected 7");
    }

    #[test]
    fn stale_drops_for_client_abandoned_requests_are_debug_not_warn() {
        // A late response for a request the client itself cancelled/superseded
        // (edit-op preemption during a drag flood) is the designed stale-drop:
        // it must not fill the console with warnings.
        let events = vec![ClientEvent::StaleResponseDropped { response_id: 5 }];

        let logs = map_client_events(events);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, UiLogLevel::Debug);
        assert_eq!(
            logs[0].message,
            "dropped stale response 5 for a request abandoned by client"
        );
    }

    fn heartbeat_event(recovery: Option<RecoveryStatus>) -> ClientEvent {
        ClientEvent::Heartbeat {
            fps: SampleStats {
                avg: 60.0,
                sdev: 0.0,
                min: 60.0,
                max: 60.0,
            },
            frame_count: 1,
            loaded_projects: Vec::new(),
            uptime_ms: 1_000,
            memory: None,
            recovery,
        }
    }

    fn recovery_status(
        level: RecoveryLevelWire,
        safe_mode: bool,
        last_crash: Option<CrashSummaryWire>,
    ) -> RecoveryStatus {
        RecoveryStatus {
            level,
            reset_reason: "power-on".to_string(),
            boot_count: 1,
            safe_mode,
            last_crash,
            paths: Vec::new(),
        }
    }
}
