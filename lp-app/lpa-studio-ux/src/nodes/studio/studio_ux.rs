use core::future::Future;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::{
    LinkConnection, LinkConnectionKind, LinkManagementRequest, LinkManagementResult,
    LinkProviderKind,
};

use crate::{
    ConnectedLink, DeviceOp, DeviceUx, LinkOpenOutcome, ProjectConnectResult, ProjectOp,
    ProjectState, ProjectUx, StudioSnapshot, StudioView, UiAction, UiActions, UiActivity, UiBody,
    UiStatus, UxActivityTarget, UxContext, UxError, UxLogEntry, UxLogLevel, UxNode, UxNotice,
    UxOutcome, UxResult, UxUpdate, UxUpdateSink,
};

pub struct StudioUx {
    device: DeviceUx,
    project: ProjectUx,
    logs: Vec<UxLogEntry>,
}

impl StudioUx {
    pub fn new() -> Self {
        Self {
            device: DeviceUx::new(),
            project: ProjectUx::new(),
            logs: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> StudioSnapshot {
        StudioSnapshot::new(
            self.device.snapshot().link,
            self.device.snapshot().server,
            self.project.snapshot(),
            self.logs.clone(),
        )
    }

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> StudioView {
        let project_snapshot = self.project.snapshot();
        let project_actions = self.project.actions(self.device.has_lightplayer_state());
        let device_view = self.device.view(&project_snapshot.state, project_actions);
        let panes = if self.project_is_loaded() {
            vec![
                self.project.view(self.device.has_lightplayer_state()),
                device_view,
            ]
        } else {
            vec![device_view]
        };
        StudioView::new(panes, self.logs.clone())
    }

    pub async fn dispatch(&mut self, action: UiAction) -> UxResult {
        self.dispatch_with_updates(action, UxUpdateSink::noop())
            .await
    }

    pub async fn dispatch_with_updates(
        &mut self,
        action: UiAction,
        updates: UxUpdateSink,
    ) -> UxResult {
        updates.emit(UxUpdate::View(self.view()));
        let result = self.dispatch_inner(action, updates.clone()).await;
        updates.emit(UxUpdate::View(self.view()));
        result
    }

    async fn dispatch_inner(&mut self, action: UiAction, updates: UxUpdateSink) -> UxResult {
        let node_id = action.node_id().clone();
        let device_node_id = self.device.node_id();
        let project_node_id = self.project.node_id();

        if node_id == device_node_id {
            let op = action.into_op::<DeviceOp>()?;
            return self.execute_device_op(op, updates).await;
        }
        if node_id == project_node_id {
            let op = action.into_op::<ProjectOp>()?;
            return self.execute_project_op(op, updates).await;
        }
        if node_id.is_descendant_of(&project_node_id) {
            return self.project.dispatch_editor_action(action, updates).await;
        }
        Err(crate::UxError::UnsupportedAction(format!(
            "unknown UX node {node_id}",
        )))
    }

    async fn execute_device_op(&mut self, op: DeviceOp, updates: UxUpdateSink) -> UxResult {
        match op {
            DeviceOp::DisconnectDevice => self.disconnect_device().await,
            DeviceOp::DisconnectLightPlayer => self.disconnect_lightplayer().await,
            DeviceOp::ConnectLightPlayer => self.connect_server_from_link(updates).await,
            DeviceOp::ProvisionFirmware => self.provision_firmware(updates).await,
            DeviceOp::ResetToBlank => self.reset_to_blank(updates).await,
            DeviceOp::RefreshConnections => {
                self.device.link.refresh_provider_catalog();
                self.device.server.disconnect();
                self.project.reset();
                Ok(UxOutcome::new().with_notice(UxNotice::info("Connection catalog refreshed")))
            }
            DeviceOp::OpenProvider { provider_id } => {
                if provider_id != LinkProviderKind::BrowserSerialEsp32 {
                    emit_activity(
                        &updates,
                        device_section_target(DeviceUx::SECTION_CONNECT_DEVICE),
                        "Opening device",
                        "Opening",
                        format!("Opening {}", provider_id.label()),
                    );
                }
                match self.device.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UxOutcome::new()),
                    LinkOpenOutcome::Cancelled { message } => {
                        Ok(UxOutcome::new().with_notice(UxNotice::info(message)))
                    }
                    LinkOpenOutcome::Connected(connected) => {
                        self.attach_connected_link(connected, updates).await
                    }
                }
            }
            DeviceOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                emit_activity(
                    &updates,
                    device_section_target(DeviceUx::SECTION_CONNECT_DEVICE),
                    "Opening device session",
                    "Connecting",
                    "Opening device endpoint",
                );
                let connected = self
                    .device
                    .link
                    .connect_endpoint(provider_id, endpoint_id)
                    .await?;
                self.attach_connected_link(connected, updates).await
            }
        }
    }

    async fn execute_project_op(&mut self, op: ProjectOp, updates: UxUpdateSink) -> UxResult {
        match op {
            ProjectOp::ConnectRunningProject => self.connect_running_project(updates).await,
            ProjectOp::ConnectLoadedProject { handle_id } => {
                self.connect_loaded_project(handle_id, updates).await
            }
            ProjectOp::LoadDemoProject => self.load_demo_project(updates).await,
            ProjectOp::DisconnectProject => self.disconnect_project().await,
        }
    }

    async fn attach_connected_link(
        &mut self,
        connected: ConnectedLink,
        updates: UxUpdateSink,
    ) -> UxResult {
        self.device.record_logs(&connected.logs);
        self.logs.extend(connected.logs);
        self.connect_server_connection(&connected.connection, updates)
            .await
    }

    async fn connect_server_from_link(&mut self, updates: UxUpdateSink) -> UxResult {
        let connection =
            self.device.link.active_connection().ok_or_else(|| {
                UxError::MissingSession("link connection is not open".to_string())
            })?;
        if should_reopen_before_server_connect(&connection) {
            self.project.reset();
            self.device.server.disconnect();
            emit_activity(
                &updates,
                device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
                "Reopening device",
                "Connecting",
                "Resetting device before server connect",
            );
            let connected = self.device.link.reopen_active_connection().await?;
            return self.attach_connected_link(connected, updates).await;
        }
        self.connect_server_connection(&connection, updates).await
    }

    async fn connect_server_connection(
        &mut self,
        connection: &LinkConnection,
        updates: UxUpdateSink,
    ) -> UxResult {
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
            "Connecting LightPlayer",
            "Connecting",
            "Opening server protocol",
        );
        let server_updates = retarget_activity_updates(
            updates.clone(),
            device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
        );
        match self.device.server.attach_link_connection(
            self.device.link.registry_handle(),
            connection,
            server_updates,
        ) {
            Ok(()) => {
                let mut outcome =
                    UxOutcome::new().with_notice(UxNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                emit_activity(
                    &updates,
                    device_section_target(DeviceUx::SECTION_OPEN_PROJECT),
                    "Checking running projects",
                    "Checking",
                    "Checking server response",
                );
                let auto_connect = match self
                    .connect_running_project_if_available(updates.clone())
                    .await
                {
                    Ok(auto_connect) => auto_connect,
                    Err(error) => {
                        let pending_logs = self.device.server.take_pending_logs();
                        self.device.record_logs(&pending_logs);
                        self.logs.extend(pending_logs);
                        self.project.reset();
                        if matches!(error, UxError::NoFirmwareDetected(_)) {
                            self.logs.push(UxLogEntry::new(
                                UxLogLevel::Info,
                                "lpa-studio-ux",
                                "No LightPlayer firmware detected during server readiness",
                            ));
                            self.device.server.fail_no_firmware();
                            return Ok(UxOutcome::new().with_notice(UxNotice::info(
                                "No LightPlayer firmware detected; flash firmware onto the selected ESP32",
                            )));
                        }
                        self.logs.push(UxLogEntry::new(
                            UxLogLevel::Error,
                            "lpa-studio-ux",
                            format!("server readiness probe failed: {error}"),
                        ));
                        self.device.server.fail(error.to_string());
                        return Err(error);
                    }
                };
                match auto_connect {
                    AutoProjectConnect::Connected => {
                        outcome = outcome.with_notice(UxNotice::info("Connected running project"));
                    }
                    AutoProjectConnect::SelectionRequired => {
                        outcome = outcome.with_notice(UxNotice::info("Choose running project"));
                    }
                    AutoProjectConnect::NotFound if should_auto_load_demo_project(connection) => {
                        let demo_outcome = self.load_demo_project(updates).await?;
                        outcome.notices.extend(demo_outcome.notices);
                    }
                    AutoProjectConnect::NotFound => {}
                }
                Ok(outcome)
            }
            Err(error) => {
                self.device.server.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn connect_running_project(&mut self, updates: UxUpdateSink) -> UxResult {
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_OPEN_PROJECT),
            "Connecting project",
            "Connecting",
            "Checking loaded projects",
        );
        let result = {
            let server = self.device.server.client_mut()?;
            self.project.connect_running_project(server).await
        };
        match result {
            Ok(ProjectConnectResult::Connected { logs }) => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Connected running project")))
            }
            Ok(ProjectConnectResult::SelectionRequired { logs }) => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Choose running project")))
            }
            Ok(ProjectConnectResult::NotFound { logs }) => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("No running project found")))
            }
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Error,
                    "lpa-studio-ux",
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn connect_running_project_if_available(
        &mut self,
        updates: UxUpdateSink,
    ) -> Result<AutoProjectConnect, UxError> {
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_OPEN_PROJECT),
            "Checking running projects",
            "Checking",
            "Checking loaded projects",
        );
        let result = {
            let server = self.device.server.client_mut()?;
            self.project
                .connect_running_project_if_available(server)
                .await
        };
        match result? {
            ProjectConnectResult::Connected { logs } => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(AutoProjectConnect::Connected)
            }
            ProjectConnectResult::SelectionRequired { logs } => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(AutoProjectConnect::SelectionRequired)
            }
            ProjectConnectResult::NotFound { logs } => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(AutoProjectConnect::NotFound)
            }
        }
    }

    async fn connect_loaded_project(&mut self, handle_id: u32, updates: UxUpdateSink) -> UxResult {
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_OPEN_PROJECT),
            "Connecting project",
            "Connecting",
            "Loading project shape",
        );
        let result = {
            let server = self.device.server.client_mut()?;
            self.project.connect_loaded_project(server, handle_id).await
        };
        match result {
            Ok(logs) => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Connected running project")))
            }
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Error,
                    "lpa-studio-ux",
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn load_demo_project(&mut self, updates: UxUpdateSink) -> UxResult {
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_OPEN_PROJECT),
            "Loading demo project",
            "Loading",
            "Uploading demo project",
        );
        let result = {
            let server = self.device.server.client_mut()?;
            self.project.load_demo_project(server).await
        };
        match result {
            Ok(logs) => {
                self.device.record_logs(&logs);
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Demo project loaded")))
            }
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Error,
                    "lpa-studio-ux",
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn disconnect_project(&mut self) -> UxResult {
        self.project.disconnect();
        Ok(UxOutcome::new().with_notice(UxNotice::info("Project disconnected")))
    }

    async fn disconnect_device(&mut self) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        self.device.link.disconnect().await?;
        Ok(UxOutcome::new().with_notice(UxNotice::info("Device disconnected")))
    }

    async fn disconnect_lightplayer(&mut self) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        Ok(UxOutcome::new().with_notice(UxNotice::info("LightPlayer disconnected")))
    }

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::FlashFirmware,
                "Flashing firmware",
                management_updates,
            )
            .await
        {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                return Err(error);
            }
        };
        self.device.record_logs(&management.logs);
        self.logs.extend(management.logs);
        let mut outcome = UxOutcome::new().with_notice(provision_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Waiting for firmware boot",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.logs.push(UxLogEntry::new(
                        UxLogLevel::Warn,
                        "lpa-studio-ux",
                        format!("firmware flashed but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UxNotice::info(
                        "Firmware flashed; reconnect the server after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Warn,
                    "lpa-studio-ux",
                    format!("firmware flashed but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UxNotice::info(
                    "Firmware flashed; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::EraseDeviceFlash,
                "Wiping device",
                management_updates,
            )
            .await
        {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                return Err(error);
            }
        };
        self.device.record_logs(&management.logs);
        self.logs.extend(management.logs);
        let mut outcome = UxOutcome::new().with_notice(reset_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceUx::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Checking for LightPlayer firmware",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.logs.push(UxLogEntry::new(
                        UxLogLevel::Warn,
                        "lpa-studio-ux",
                        format!("device wiped but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UxNotice::info(
                        "Device wiped; reconnect after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Warn,
                    "lpa-studio-ux",
                    format!("device wiped but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UxNotice::info(
                    "Device wiped; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
    }

    fn record_logs(&mut self, logs: Vec<UxLogEntry>) {
        if logs.is_empty() {
            return;
        }
        self.device.record_logs(&logs);
        self.logs.extend(logs);
    }
}

impl Default for StudioUx {
    fn default() -> Self {
        Self::new()
    }
}

impl UxContext for StudioUx {
    fn dispatch(
        &mut self,
        action: UiAction,
    ) -> core::pin::Pin<Box<dyn Future<Output = UxResult> + '_>> {
        Box::pin(StudioUx::dispatch(self, action))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoProjectConnect {
    Connected,
    SelectionRequired,
    NotFound,
}

fn should_auto_load_demo_project(connection: &LinkConnection) -> bool {
    matches!(connection.kind, LinkConnectionKind::BrowserWorker { .. })
}

fn emit_activity(
    updates: &UxUpdateSink,
    target: UxActivityTarget,
    title: impl Into<String>,
    status: impl Into<String>,
    detail: impl Into<String>,
) {
    updates.emit(UxUpdate::Activity {
        target,
        status: UiStatus::working(status),
        activity: UiActivity::new(title).with_detail(detail),
    });
}

fn device_section_target(section_id: &'static str) -> UxActivityTarget {
    UxActivityTarget::stack_section(DeviceUx::NODE_ID, section_id)
}

fn retarget_activity_updates(updates: UxUpdateSink, target: UxActivityTarget) -> UxUpdateSink {
    UxUpdateSink::new(move |update| match update {
        UxUpdate::Activity {
            status, activity, ..
        } => updates.emit(UxUpdate::Activity {
            target: target.clone(),
            status,
            activity,
        }),
        update => updates.emit(update),
    })
}

fn capture_log_updates(
    updates: UxUpdateSink,
    captured_logs: Rc<RefCell<Vec<UxLogEntry>>>,
) -> UxUpdateSink {
    UxUpdateSink::new(move |update| {
        if let UxUpdate::Log(log) = &update {
            captured_logs.borrow_mut().push(log.clone());
        }
        updates.emit(update);
    })
}

fn view_actions(view: &StudioView) -> Vec<UiAction> {
    let mut actions = Vec::new();
    for pane in &view.panes {
        actions.extend(pane.actions.clone());
        actions.extend(body_actions(&pane.body));
    }
    actions
}

fn body_actions(body: &UiBody) -> Vec<UiAction> {
    match body {
        UiBody::Stack(stack) => stack
            .sections
            .iter()
            .flat_map(|section| {
                let mut actions = section.actions.clone();
                actions.extend(body_actions(&section.body));
                actions
            })
            .collect(),
        UiBody::Empty
        | UiBody::Text(_)
        | UiBody::Progress(_)
        | UiBody::Activity(_)
        | UiBody::Issue(_)
        | UiBody::Metrics(_) => Vec::new(),
    }
}

fn should_reopen_before_server_connect(connection: &LinkConnection) -> bool {
    matches!(
        connection.kind,
        LinkConnectionKind::BrowserSerialEsp32 { .. }
    )
}

fn provision_notice(result: &LinkManagementResult) -> UxNotice {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            UxNotice::info(format!("Flashed {}", result.manifest.display_name))
        }
        _ => UxNotice::info("Firmware flashed"),
    }
}

fn reset_notice(result: &LinkManagementResult) -> UxNotice {
    match result {
        LinkManagementResult::EraseDeviceFlash(result) => {
            let label = result.chip_name.as_deref().unwrap_or("selected ESP32");
            UxNotice::info(format!("{label} wiped"))
        }
        _ => UxNotice::info("Device wiped"),
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use std::cell::RefCell;
    use std::rc::Rc;

    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{
        LinkCapabilities, LinkConnection, LinkConnectionKind, LinkEndpoint, LinkEndpointId,
        LinkProviderKind, LinkSession,
    };

    use crate::{
        ConnectedDeviceSummary, LinkState, LinkUx, ProjectEditorOp, ProjectEditorTarget,
        ProjectInventorySummary, ProjectState, ProjectUx, ServerFailureKind, ServerState, ServerUx,
        UiStatusKind, UiStepState, UxIssue, UxNodeId,
    };

    use super::*;

    #[test]
    fn initial_snapshot_selects_provider() {
        let studio = StudioUx::new();

        assert!(matches!(
            studio.snapshot().link.state,
            LinkState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn initial_actions_target_device_node() {
        let studio = StudioUx::new();

        let actions = studio.actions();

        assert!(
            actions
                .iter()
                .all(|action| action.node_id().as_str() == DeviceUx::NODE_ID)
        );
    }

    #[test]
    fn initial_view_exposes_device_pane() {
        let studio = StudioUx::new();

        let view = studio.view();

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceUx::NODE_ID);
        assert_eq!(device_section_ids(&view), vec!["select-connection"]);
    }

    #[test]
    fn connected_without_project_keeps_project_actions_in_device_pane() {
        let mut studio = connected_studio();
        studio.project.reset();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceUx::NODE_ID);
        assert_eq!(
            device_section_ids(&view),
            vec![
                "select-connection",
                "connect-device",
                "connect-lightplayer",
                "open-project"
            ]
        );
        assert!(actions.iter().any(|action| {
            matches!(
                action.op_as::<ProjectOp>(),
                Some(ProjectOp::ConnectRunningProject)
            )
        }));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::LoadDemoProject)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ResetToBlank | DeviceOp::DisconnectDevice)
        )));
    }

    #[test]
    fn connected_link_without_server_hides_open_project_step() {
        let studio = link_connected_studio();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(
            device_section_ids(&view),
            vec!["select-connection", "connect-device", "connect-lightplayer"]
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
    }

    #[test]
    fn no_firmware_failure_hides_connect_lightplayer_action() {
        let mut studio = connected_studio();
        studio.project.reset();
        studio
            .device
            .link
            .set_active_session_for_test(management_capable_session());
        studio.device.server.set_state(ServerState::Failed {
            issue: UxIssue::new("No LightPlayer firmware detected."),
            kind: ServerFailureKind::NoFirmware,
        });

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes[0].status.kind, UiStatusKind::Warning);
        assert_eq!(view.panes[0].status.label, "Ready to flash");
        assert_eq!(
            device_section_ids(&view),
            vec!["select-connection", "connect-device", "connect-lightplayer"]
        );
        let UiBody::Stack(stack) = &view.panes[0].body else {
            panic!("device pane should render a stack view");
        };
        let lightplayer_section = stack
            .sections
            .iter()
            .find(|section| section.id == "connect-lightplayer")
            .expect("connect lightplayer section should exist");
        assert_eq!(lightplayer_section.title, "LightPlayer unavailable");
        assert_eq!(lightplayer_section.state, UiStepState::Active);
        assert!(matches!(lightplayer_section.body, UiBody::Text(_)));
        let device_section = stack
            .sections
            .iter()
            .find(|section| section.id == "connect-device")
            .expect("connect device section should exist");
        assert!(device_section.actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            device_section
                .actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
    }

    #[test]
    fn loaded_project_gets_project_pane() {
        let studio = connected_studio();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 2);
        assert_eq!(view.panes[0].node_id.as_str(), ProjectUx::NODE_ID);
        assert_eq!(view.panes[1].node_id.as_str(), DeviceUx::NODE_ID);
        assert_eq!(
            device_section_ids(&view),
            vec![
                "select-connection",
                "connect-device",
                "connect-lightplayer",
                "open-project"
            ]
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ResetToBlank | DeviceOp::DisconnectDevice)
        )));
    }

    #[test]
    fn project_disconnect_leaves_server_and_link_connected() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_project()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.device.server.snapshot().state,
            ServerState::Connected { .. }
        ));
        assert!(matches!(
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
        ));
    }

    #[test]
    fn lightplayer_disconnect_leaves_device_link_connected() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_lightplayer()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.device.server.snapshot().state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
        ));
        let actions = view_actions(&studio.view());
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
    }

    #[test]
    fn device_disconnect_clears_project_server_and_link() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_device()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.device.server.snapshot().state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.device.link.snapshot().state,
            LinkState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn device_action_dispatch_routes_exact_device_target() {
        let mut studio = connected_studio();
        let action =
            UiAction::from_op(UxNodeId::new(DeviceUx::NODE_ID), DeviceOp::DisconnectDevice);

        block_on_ready(studio.dispatch(action)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.device.server.snapshot().state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.device.link.snapshot().state,
            LinkState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn project_action_dispatch_routes_exact_project_target() {
        let mut studio = connected_studio();
        let action = UiAction::from_op(
            UxNodeId::new(ProjectUx::NODE_ID),
            ProjectOp::DisconnectProject,
        );

        block_on_ready(studio.dispatch(action)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.device.server.snapshot().state,
            ServerState::Connected { .. }
        ));
        assert!(matches!(
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
        ));
    }

    #[test]
    fn project_descendant_action_dispatch_routes_to_project_ux() {
        let mut studio = StudioUx::new();
        let target = ProjectEditorTarget::node_tree();
        let action = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);

        block_on_ready(studio.dispatch(action)).unwrap();

        assert_eq!(studio.project.active_editor_target(), Some(&target));
    }

    #[test]
    fn unknown_top_level_dispatch_fails_clearly() {
        let mut studio = StudioUx::new();
        let action = UiAction::from_op(UxNodeId::new("studio.unknown"), ProjectEditorOp::Focus);

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UxError::UnsupportedAction(message))
                if message.contains("unknown UX node studio.unknown")
        ));
    }

    #[test]
    fn unknown_project_descendant_dispatch_fails_as_project_target() {
        let mut studio = StudioUx::new();
        let action = UiAction::from_op(
            UxNodeId::new("studio.project.unknown"),
            ProjectEditorOp::Focus,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UxError::UnsupportedAction(message))
                if message.contains("unknown project editor target studio.project.unknown")
        ));
    }

    #[test]
    fn project_descendant_dispatch_rejects_wrong_op_type() {
        let mut studio = StudioUx::new();
        let action = UiAction::from_op(
            ProjectEditorTarget::node_tree().node_id(),
            ProjectOp::LoadDemoProject,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UxError::UnsupportedAction(message))
                if message.contains("ProjectEditorOp")
        ));
    }

    #[test]
    fn failed_link_dispatch_emits_final_failed_view_after_activity() {
        let mut studio = StudioUx::new();
        studio.device.link = LinkUx::with_registry(registry_with_fake_connect_error(
            "Failed to open serial port.",
        ));
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| {
                updates.borrow_mut().push(update);
            }
        });
        let action = UiAction::from_op(
            UxNodeId::new(DeviceUx::NODE_ID),
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::Fake,
                endpoint_id: LinkEndpointId::new("fake-runtime"),
            },
        );

        let result = block_on_ready(studio.dispatch_with_updates(action, sink));

        assert!(matches!(result, Err(UxError::Link(_))));
        assert!(updates.borrow().iter().any(|update| {
            matches!(
                update,
                UxUpdate::Activity {
                    target: UxActivityTarget::StackSection {
                        pane_node_id,
                        section_id,
                    },
                    activity,
                    ..
                } if pane_node_id.as_str() == DeviceUx::NODE_ID
                    && section_id == DeviceUx::SECTION_CONNECT_DEVICE
                    && activity.title == "Opening device session"
            )
        }));
        let last_view = updates
            .borrow()
            .iter()
            .rev()
            .find_map(|update| match update {
                UxUpdate::View(view) => Some(view.clone()),
                _ => None,
            })
            .expect("dispatch should emit a final view");
        assert_eq!(last_view.panes[0].status.kind, UiStatusKind::Error);
        assert_eq!(last_view.panes[0].status.label, "Needs attention");
    }

    #[test]
    fn only_browser_worker_connections_auto_load_demo_project() {
        let browser_worker = LinkConnection::browser_worker("browser-worker-worker-1", "session-1");
        let fake = LinkConnection::fake("fake-runtime", "fake-session");

        assert!(should_auto_load_demo_project(&browser_worker));
        assert!(!should_auto_load_demo_project(&fake));
    }

    #[test]
    fn retarget_activity_updates_rewrites_activity_target() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| {
                updates.borrow_mut().push(update);
            }
        });
        let target = UxActivityTarget::stack_section(
            DeviceUx::NODE_ID,
            DeviceUx::SECTION_CONNECT_LIGHTPLAYER,
        );
        let retargeted = retarget_activity_updates(sink, target.clone());

        retargeted.emit(UxUpdate::Activity {
            target: UxActivityTarget::pane(ServerUx::NODE_ID),
            status: UiStatus::working("Connecting"),
            activity: UiActivity::new("Connecting ESP32 server"),
        });

        assert!(matches!(
            updates.borrow().as_slice(),
            [UxUpdate::Activity {
                target: actual_target,
                ..
            }] if *actual_target == target
        ));
    }

    fn connected_studio() -> StudioUx {
        let mut studio = link_connected_studio();
        studio.device.server.set_state(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn link_connected_studio() -> StudioUx {
        let mut studio = StudioUx::new();
        studio.device.link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
        studio
    }

    fn device_section_ids(view: &StudioView) -> Vec<&str> {
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceUx::NODE_ID)
            .expect("device pane should exist");
        let UiBody::Stack(stack) = &device_pane.body else {
            panic!("device pane should render stack");
        };
        stack
            .sections
            .iter()
            .map(|section| section.id.as_str())
            .collect()
    }

    fn registry_with_fake_connect_error(message: impl Into<String>) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(
            FakeProvider::new()
                .with_endpoint(LinkEndpoint::new(
                    "fake-runtime",
                    LinkProviderKind::Fake,
                    "Fake runtime",
                ))
                .with_connect_error(message),
        );
        registry
    }

    fn management_capable_session() -> LinkSession {
        LinkSession::new(
            "fake-session",
            LinkProviderKind::Fake,
            "fake-runtime",
            LinkConnectionKind::Fake,
            LinkCapabilities::esp32_serial_base()
                .with_flash()
                .with_device_erase(),
        )
    }

    fn block_on_ready<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
