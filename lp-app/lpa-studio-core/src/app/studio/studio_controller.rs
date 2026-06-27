use core::future::Future;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::{
    LinkConnection, LinkConnectionKind, LinkManagementRequest, LinkManagementResult,
    LinkProviderKind,
};

use crate::core::notice::UiNotices;
use crate::{
    ConnectedLink, Controller, ControllerContext, DeviceController, DeviceOp, LinkOpenOutcome,
    ProjectConnectResult, ProjectController, ProjectOp, ProjectState, ProjectSyncRun,
    StudioSnapshot, UiAction, UiActions, UiActivityView, UiError, UiLogEntry, UiLogLevel, UiNotice,
    UiResult, UiStatus, UiStudioView, UiViewContent, UxActivityTarget, UxUpdate, UxUpdateSink,
};

pub struct StudioController {
    device: DeviceController,
    project: ProjectController,
    logs: Vec<UiLogEntry>,
}

impl StudioController {
    pub fn new() -> Self {
        Self {
            device: DeviceController::new(),
            project: ProjectController::new(),
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

    pub fn view(&self) -> UiStudioView {
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
        UiStudioView::new(panes, self.logs.clone())
    }

    pub async fn dispatch(&mut self, action: UiAction) -> UiResult {
        self.dispatch_with_updates(action, UxUpdateSink::noop())
            .await
    }

    pub async fn dispatch_with_updates(
        &mut self,
        action: UiAction,
        updates: UxUpdateSink,
    ) -> UiResult {
        updates.emit(UxUpdate::View(self.view()));
        let result = self.dispatch_inner(action, updates.clone()).await;
        updates.emit(UxUpdate::View(self.view()));
        result
    }

    /// Refresh a loaded project for passive UI updates.
    ///
    /// This bypasses the generic action activity/notice path so the web shell can
    /// keep selected visual-product previews fresh without showing a user action
    /// as running.
    pub async fn refresh_loaded_project_tick(&mut self) -> Result<Option<ProjectSyncRun>, UiError> {
        if !self.project_is_loaded() || !self.device.has_lightplayer_state() {
            return Ok(None);
        }
        let sync = {
            let server = self.device.server.client_mut()?;
            self.project.refresh_project(server).await?
        };
        self.record_project_sync_run(&sync);
        Ok(Some(sync))
    }

    pub fn mark_passive_project_refresh_failed(&mut self, message: impl Into<String>) {
        self.project.mark_project_sync_failed(message);
    }

    pub fn disable_control_product_probes(&mut self, reason: impl Into<String>) -> bool {
        self.project.disable_control_product_probes(reason)
    }

    pub fn recover_from_foreground_action_timeout(
        &mut self,
        message: impl Into<String>,
        fail_server: bool,
    ) {
        let message = message.into();
        self.project.mark_project_sync_failed(message.clone());
        if fail_server {
            self.device.server.fail(message);
        }
    }

    async fn dispatch_inner(&mut self, action: UiAction, updates: UxUpdateSink) -> UiResult {
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
            let outcome = self
                .project
                .dispatch_editor_action(action, updates.clone())
                .await?;
            updates.emit(UxUpdate::View(self.view()));
            if self.project_is_loaded() && self.device.is_lightplayer_connected() {
                let sync = {
                    let server = self.device.server.client_mut()?;
                    self.project.refresh_project(server).await?
                };
                self.record_project_sync_run(&sync);
                updates.emit(UxUpdate::View(self.view()));
            }
            return Ok(outcome);
        }
        Err(crate::UiError::UnsupportedAction(format!(
            "unknown UX node {node_id}",
        )))
    }

    async fn execute_device_op(&mut self, op: DeviceOp, updates: UxUpdateSink) -> UiResult {
        match op {
            DeviceOp::DisconnectDevice => self.disconnect_device().await,
            DeviceOp::DisconnectLightPlayer => self.disconnect_lightplayer().await,
            DeviceOp::ResetDevice => self.reset_device(updates).await,
            DeviceOp::ConnectLightPlayer => self.connect_server_from_link(updates).await,
            DeviceOp::ProvisionFirmware => self.provision_firmware(updates).await,
            DeviceOp::ResetToBlank => self.reset_to_blank(updates).await,
            DeviceOp::RefreshConnections => {
                self.device.link.refresh_provider_catalog();
                self.device.server.disconnect();
                self.project.reset();
                Ok(UiNotices::new().with_notice(UiNotice::info("Connection catalog refreshed")))
            }
            DeviceOp::OpenProvider { provider_id } => {
                if provider_id != LinkProviderKind::BrowserSerialEsp32 {
                    emit_activity(
                        &updates,
                        device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
                        "Opening device",
                        "Opening",
                        format!("Opening {}", provider_id.label()),
                    );
                }
                match self.device.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UiNotices::new()),
                    LinkOpenOutcome::Cancelled { message } => {
                        Ok(UiNotices::new().with_notice(UiNotice::info(message)))
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
                    device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
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

    async fn execute_project_op(&mut self, op: ProjectOp, updates: UxUpdateSink) -> UiResult {
        match op {
            ProjectOp::ConnectRunningProject => self.connect_running_project(updates).await,
            ProjectOp::ConnectLoadedProject { handle_id } => {
                self.connect_loaded_project(handle_id, updates).await
            }
            ProjectOp::LoadDemoProject => self.load_demo_project(updates).await,
            ProjectOp::RefreshProject => self.refresh_project(updates).await,
            ProjectOp::DisconnectProject => self.disconnect_project().await,
        }
    }

    async fn attach_connected_link(
        &mut self,
        connected: ConnectedLink,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.logs.extend(connected.logs);
        self.connect_server_connection(&connected.connection, updates)
            .await
    }

    async fn connect_server_from_link(&mut self, updates: UxUpdateSink) -> UiResult {
        let connection =
            self.device.link.active_connection().ok_or_else(|| {
                UiError::MissingSession("link connection is not open".to_string())
            })?;
        if should_reopen_before_server_connect(&connection) {
            self.project.reset();
            self.device.server.disconnect();
            emit_activity(
                &updates,
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
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
    ) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Connecting LightPlayer",
            "Connecting",
            "Opening server protocol",
        );
        let server_updates = retarget_activity_updates(
            updates.clone(),
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
        );
        match self.device.server.attach_link_connection(
            self.device.link.registry_handle(),
            connection,
            server_updates,
        ) {
            Ok(()) => {
                let mut outcome =
                    UiNotices::new().with_notice(UiNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                emit_activity(
                    &updates,
                    device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                        self.logs.extend(pending_logs);
                        self.project.reset();
                        if matches!(error, UiError::NoFirmwareDetected(_)) {
                            self.logs.push(UiLogEntry::new(
                                UiLogLevel::Info,
                                "lpa-studio-core",
                                "No LightPlayer firmware detected during server readiness",
                            ));
                            self.device.server.fail_no_firmware();
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "No LightPlayer firmware detected; flash firmware onto the selected ESP32",
                            )));
                        }
                        self.logs.push(UiLogEntry::new(
                            UiLogLevel::Error,
                            "lpa-studio-core",
                            format!("server readiness probe failed: {error}"),
                        ));
                        self.device.server.fail(error.to_string());
                        return Err(error);
                    }
                };
                match auto_connect {
                    AutoProjectConnect::Connected { synced } => {
                        outcome = outcome.with_notice(project_sync_notice(
                            synced,
                            "Connected running project",
                            "Connected running project; project sync needs attention",
                        ));
                    }
                    AutoProjectConnect::SelectionRequired => {
                        outcome = outcome.with_notice(UiNotice::info("Choose running project"));
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

    async fn connect_running_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                self.logs.extend(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Connected running project",
                    "Connected running project; project sync needs attention",
                )))
            }
            Ok(ProjectConnectResult::SelectionRequired { logs }) => {
                self.logs.extend(logs);
                Ok(UiNotices::new().with_notice(UiNotice::info("Choose running project")))
            }
            Ok(ProjectConnectResult::NotFound { logs }) => {
                self.logs.extend(logs);
                Ok(UiNotices::new().with_notice(UiNotice::info("No running project found")))
            }
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Error,
                    "lpa-studio-core",
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
    ) -> Result<AutoProjectConnect, UiError> {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                self.logs.extend(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(AutoProjectConnect::Connected {
                    synced: sync.synced,
                })
            }
            ProjectConnectResult::SelectionRequired { logs } => {
                self.logs.extend(logs);
                Ok(AutoProjectConnect::SelectionRequired)
            }
            ProjectConnectResult::NotFound { logs } => {
                self.logs.extend(logs);
                Ok(AutoProjectConnect::NotFound)
            }
        }
    }

    async fn connect_loaded_project(&mut self, handle_id: u32, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                self.logs.extend(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Connected running project",
                    "Connected running project; project sync needs attention",
                )))
            }
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Error,
                    "lpa-studio-core",
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn load_demo_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                self.logs.extend(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Demo project loaded",
                    "Demo project loaded; project sync needs attention",
                )))
            }
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Error,
                    "lpa-studio-core",
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn disconnect_project(&mut self) -> UiResult {
        self.project.disconnect();
        Ok(UiNotices::new().with_notice(UiNotice::info("Project disconnected")))
    }

    async fn refresh_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Refreshing project",
            "Refreshing",
            "Reading project state",
        );
        updates.emit(UxUpdate::View(self.view()));
        let sync = {
            let server = self.device.server.client_mut()?;
            self.project.refresh_project(server).await?
        };
        self.record_project_sync_run(&sync);
        updates.emit(UxUpdate::View(self.view()));
        Ok(UiNotices::new().with_notice(project_sync_notice(
            sync.synced,
            "Project refreshed",
            "Project refresh needs attention",
        )))
    }

    async fn sync_project_after_attach(
        &mut self,
        updates: UxUpdateSink,
    ) -> Result<ProjectSyncRun, UiError> {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Syncing project",
            "Syncing",
            "Reading project state",
        );
        updates.emit(UxUpdate::View(self.view()));
        let sync = {
            let server = self.device.server.client_mut()?;
            self.project.sync_loaded_project(server).await?
        };
        self.record_project_sync_run(&sync);
        updates.emit(UxUpdate::View(self.view()));
        Ok(sync)
    }

    fn record_project_sync_run(&mut self, sync: &ProjectSyncRun) {
        self.logs.extend(sync.logs.clone());
    }

    async fn disconnect_device(&mut self) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        self.device.link.disconnect().await?;
        Ok(UiNotices::new().with_notice(UiNotice::info("Device disconnected")))
    }

    async fn disconnect_lightplayer(&mut self) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        Ok(UiNotices::new().with_notice(UiNotice::info("LightPlayer disconnected")))
    }

    async fn reset_device(&mut self, updates: UxUpdateSink) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::ResetRuntime,
                "Resetting device",
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
        self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
        self.logs.extend(management.logs);

        let mut outcome = UiNotices::new().with_notice(UiNotice::info("Device reset"));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Waiting for device boot",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.logs.push(UiLogEntry::new(
                        UiLogLevel::Warn,
                        "lpa-studio-core",
                        format!("device reset but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Device reset; reconnect after it finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "lpa-studio-core",
                    format!("device reset but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Device reset; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
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
        self.logs.extend(management.logs);
        let mut outcome = UiNotices::new().with_notice(provision_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
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
                    self.logs.push(UiLogEntry::new(
                        UiLogLevel::Warn,
                        "lpa-studio-core",
                        format!("firmware flashed but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Firmware flashed; reconnect the server after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "lpa-studio-core",
                    format!("firmware flashed but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Firmware flashed; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
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
        self.logs.extend(management.logs);
        let mut outcome = UiNotices::new().with_notice(reset_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
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
                    self.logs.push(UiLogEntry::new(
                        UiLogLevel::Warn,
                        "lpa-studio-core",
                        format!("device wiped but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Device wiped; reconnect after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "lpa-studio-core",
                    format!("device wiped but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Device wiped; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
    }

    fn record_logs(&mut self, logs: Vec<UiLogEntry>) {
        if logs.is_empty() {
            return;
        }
        self.logs.extend(logs);
    }
}

impl Default for StudioController {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllerContext for StudioController {
    fn dispatch(
        &mut self,
        action: UiAction,
    ) -> core::pin::Pin<Box<dyn Future<Output = UiResult> + '_>> {
        Box::pin(StudioController::dispatch(self, action))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoProjectConnect {
    Connected { synced: bool },
    SelectionRequired,
    NotFound,
}

fn project_sync_notice(synced: bool, success: &str, needs_attention: &str) -> UiNotice {
    if synced {
        UiNotice::info(success)
    } else {
        UiNotice::warning(needs_attention)
    }
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
        activity: UiActivityView::new(title).with_detail(detail),
    });
}

fn device_section_target(section_id: &'static str) -> UxActivityTarget {
    UxActivityTarget::stack_section(DeviceController::NODE_ID, section_id)
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
    captured_logs: Rc<RefCell<Vec<UiLogEntry>>>,
) -> UxUpdateSink {
    UxUpdateSink::new(move |update| {
        if let UxUpdate::Log(log) = &update {
            captured_logs.borrow_mut().push(log.clone());
        }
        updates.emit(update);
    })
}

fn view_actions(view: &UiStudioView) -> Vec<UiAction> {
    let mut actions = Vec::new();
    for pane in &view.panes {
        actions.extend(pane.actions.clone());
        actions.extend(body_actions(&pane.body));
    }
    actions
}

fn body_actions(body: &UiViewContent) -> Vec<UiAction> {
    match body {
        UiViewContent::Stack(stack) => stack
            .sections
            .iter()
            .flat_map(|section| {
                let mut actions = section.actions.clone();
                actions.extend(body_actions(&section.body));
                actions
            })
            .collect(),
        UiViewContent::Empty
        | UiViewContent::Text(_)
        | UiViewContent::Progress(_)
        | UiViewContent::Activity(_)
        | UiViewContent::Issue(_)
        | UiViewContent::Metrics(_) => Vec::new(),
        UiViewContent::ProjectEditor(editor) => editor
            .tree
            .roots
            .iter()
            .flat_map(project_tree_item_actions)
            .collect(),
    }
}

fn project_tree_item_actions(
    item: &crate::ProjectNodeTreeItem,
) -> Box<dyn Iterator<Item = UiAction> + '_> {
    Box::new(
        core::iter::once(item.action.clone())
            .chain(item.children.iter().flat_map(project_tree_item_actions)),
    )
}

fn should_reopen_before_server_connect(connection: &LinkConnection) -> bool {
    matches!(
        connection.kind,
        LinkConnectionKind::BrowserSerialEsp32 { .. }
    )
}

fn provision_notice(result: &LinkManagementResult) -> UiNotice {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            UiNotice::info(format!("Flashed {}", result.manifest.display_name))
        }
        _ => UiNotice::info("Firmware flashed"),
    }
}

fn reset_notice(result: &LinkManagementResult) -> UiNotice {
    match result {
        LinkManagementResult::EraseDeviceFlash(result) => {
            let label = result.chip_name.as_deref().unwrap_or("selected ESP32");
            UiNotice::info(format!("{label} wiped"))
        }
        _ => UiNotice::info("Device wiped"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use std::cell::RefCell;
    use std::rc::Rc;

    use lpa_client::ClientIo;
    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{
        LinkCapabilities, LinkConnection, LinkConnectionKind, LinkEndpoint, LinkEndpointId,
        LinkProviderKind, LinkSession,
    };
    use lpc_model::{
        LpType, LpValue, NodeId, ProductKind, ProductRef, Revision, SlotData, SlotFieldShape,
        SlotMeta, SlotRecord, SlotShape, SlotShapeId, TreePath, VisualProduct, WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{
        ClientMessage, ClientRequest, MemoryStats, NodeRuntimeStatus, ProjectProbeRequest,
        ProjectReadEvent, ProjectReadFrame, ProjectReadQueryEvent, ProjectRuntimeStatus,
        RenderProductProbeRequest, RuntimeReadResult, ServerRuntimeStatus, TransportError,
        WireEntryState, WireServerMessage, WireServerMsgBody, WireTextureFormat,
    };

    use super::*;
    use crate::core::status::UiStatusKind;
    use crate::core::view::steps_view::UiStepState;
    use crate::{
        ConnectedDeviceSummary, ControllerId, LinkController, LinkState, ProjectController,
        ProjectEditorOp, ProjectEditorTarget, ProjectInventorySummary, ProjectNodeAddress,
        ProjectNodeTarget, ProjectState, ProjectSyncPhase, ServerController, ServerFailureKind,
        ServerState, StudioServerClient, UiIssue, UiProductPreviewFrame,
    };

    #[test]
    fn initial_snapshot_selects_provider() {
        let studio = StudioController::new();

        assert!(matches!(
            studio.snapshot().link.state,
            LinkState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn initial_actions_target_device_node() {
        let studio = StudioController::new();

        let actions = studio.actions();

        assert!(
            actions
                .iter()
                .all(|action| action.node_id().as_str() == DeviceController::NODE_ID)
        );
    }

    #[test]
    fn initial_view_exposes_device_pane() {
        let studio = StudioController::new();

        let view = studio.view();

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceController::NODE_ID);
        assert_eq!(device_section_ids(&view), vec!["select-connection"]);
    }

    #[test]
    fn connected_without_project_keeps_project_actions_in_device_pane() {
        let mut studio = connected_studio();
        studio.project.reset();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceController::NODE_ID);
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
            issue: UiIssue::new("No LightPlayer firmware detected."),
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
        let UiViewContent::Stack(stack) = &view.panes[0].body else {
            panic!("device pane should render a stack view");
        };
        let lightplayer_section = stack
            .sections
            .iter()
            .find(|section| section.id == "connect-lightplayer")
            .expect("connect lightplayer section should exist");
        assert_eq!(lightplayer_section.title, "LightPlayer unavailable");
        assert_eq!(lightplayer_section.state, UiStepState::Active);
        assert!(matches!(lightplayer_section.body, UiViewContent::Text(_)));
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
        assert_eq!(view.panes[0].node_id.as_str(), ProjectController::NODE_ID);
        assert_eq!(view.panes[1].node_id.as_str(), DeviceController::NODE_ID);
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
    fn connected_lightplayer_offers_non_destructive_device_reset() {
        let mut studio = connected_studio();
        studio
            .device
            .link
            .set_active_session_for_test(management_capable_session());

        let actions = view_actions(&studio.view());

        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetDevice)))
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
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
        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::DisconnectDevice,
        );

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
            ControllerId::new(ProjectController::NODE_ID),
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
    fn refresh_project_dispatch_reads_project_and_updates_sync_summary() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![project_read_response_with_runtime(1, Revision::new(13))],
        );
        let mut studio = connected_studio_with_client(io);
        let action = UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::RefreshProject,
        );

        let outcome = block_on_ready(studio.dispatch(action)).unwrap();

        assert!(
            outcome
                .notices
                .iter()
                .any(|notice| notice.message == "Project refreshed")
        );
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        let ClientRequest::ProjectRead { handle, request } = &sent[0].msg else {
            panic!("refresh should send a project read request");
        };
        assert_eq!(sent[0].id, 1);
        assert_eq!(handle.id(), 7);
        assert_eq!(request.since, None);
        assert_eq!(request.queries.len(), 4);

        let sync = studio
            .project
            .snapshot()
            .sync
            .expect("refresh should leave a sync summary");
        assert_eq!(sync.phase, ProjectSyncPhase::Ready);
        assert_eq!(sync.revision, 13);
        assert_eq!(
            sync.runtime.as_ref().map(|runtime| runtime.frame_num),
            Some(77)
        );
        assert_eq!(
            sync.runtime.as_ref().and_then(|runtime| runtime.free_bytes),
            Some(4096)
        );
    }

    #[test]
    fn project_descendant_action_dispatch_routes_to_project_ux() {
        let mut studio = StudioController::new();
        let target = ProjectEditorTarget::node_tree();
        let action = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);

        block_on_ready(studio.dispatch(action)).unwrap();

        assert_eq!(studio.project.active_editor_target(), Some(&target));
    }

    #[test]
    fn project_node_focus_dispatch_requests_visual_product_preview() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![project_read_response_with_runtime(1, Revision::new(13))],
        );
        let mut studio = connected_studio_with_client(io);
        studio
            .project
            .apply_project_view(&single_product_project_view(3))
            .unwrap();
        let product = VisualProduct::new(NodeId::new(3), 0);
        let target = ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
            ProjectNodeAddress::new(TreePath::parse("/demo.project/orbit.shader").unwrap()),
            NodeId::new(3),
        ));
        let action = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);

        block_on_ready(studio.dispatch(action)).unwrap();

        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        let ClientRequest::ProjectRead { request, .. } = &sent[0].msg else {
            panic!("node focus should send a project read request");
        };
        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::RenderProduct(
                RenderProductProbeRequest {
                    product,
                    width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                    height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                    format: WireTextureFormat::Srgb8,
                },
            )]
        );
    }

    #[test]
    fn unknown_top_level_dispatch_fails_clearly() {
        let mut studio = StudioController::new();
        let action = UiAction::from_op(ControllerId::new("studio|unknown"), ProjectEditorOp::Focus);

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("unknown UX node studio|unknown")
        ));
    }

    #[test]
    fn unknown_project_descendant_dispatch_fails_as_project_target() {
        let mut studio = StudioController::new();
        let action = UiAction::from_op(
            ControllerId::new("studio|project|unknown"),
            ProjectEditorOp::Focus,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("unknown project editor target studio|project|unknown")
        ));
    }

    #[test]
    fn project_descendant_dispatch_rejects_wrong_op_type() {
        let mut studio = StudioController::new();
        let action = UiAction::from_op(
            ProjectEditorTarget::node_tree().node_id(),
            ProjectOp::LoadDemoProject,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("ProjectEditorOp")
        ));
    }

    #[test]
    fn failed_link_dispatch_emits_final_failed_view_after_activity() {
        let mut studio = StudioController::new();
        studio.device.link = LinkController::with_registry(registry_with_fake_connect_error(
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
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::Fake,
                endpoint_id: LinkEndpointId::new("fake-runtime"),
            },
        );

        let result = block_on_ready(studio.dispatch_with_updates(action, sink));

        assert!(matches!(result, Err(UiError::Link(_))));
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
                } if pane_node_id.as_str() == DeviceController::NODE_ID
                    && section_id == DeviceController::SECTION_CONNECT_DEVICE
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
            DeviceController::NODE_ID,
            DeviceController::SECTION_CONNECT_LIGHTPLAYER,
        );
        let retargeted = retarget_activity_updates(sink, target.clone());

        retargeted.emit(UxUpdate::Activity {
            target: UxActivityTarget::pane(ServerController::NODE_ID),
            status: UiStatus::working("Connecting"),
            activity: UiActivityView::new("Connecting ESP32 server"),
        });

        assert!(matches!(
            updates.borrow().as_slice(),
            [UxUpdate::Activity {
                target: actual_target,
                ..
            }] if *actual_target == target
        ));
    }

    fn connected_studio() -> StudioController {
        let mut studio = link_connected_studio();
        studio.device.server.set_state(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn connected_studio_with_client(io: ScriptedClientIo) -> StudioController {
        let mut studio = link_connected_studio();
        studio
            .device
            .server
            .set_client_for_test(StudioServerClient::from_io_for_test(
                "fake-protocol",
                Box::new(io),
            ));
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn link_connected_studio() -> StudioController {
        let mut studio = StudioController::new();
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

    fn single_product_project_view(node_id: u32) -> ProjectView {
        let revision = Revision::new(1);
        let path = TreePath::parse("/demo.project/orbit.shader").unwrap();
        let state_shape = SlotShapeId::new(700);
        let mut view = ProjectView::new();
        view.tree.insert(TreeEntryView::new(
            NodeId::new(node_id),
            path,
            None,
            None,
            NodeRuntimeStatus::Ok,
            WireEntryState::Alive,
            revision,
            revision,
            revision,
        ));
        view.slots
            .registry
            .register_dynamic_shape(
                state_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new(
                            "output",
                            SlotShape::value(LpType::Product(ProductKind::Visual)),
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::Product(ProductRef::visual(VisualProduct::new(
                        NodeId::new(node_id),
                        0,
                    ))),
                ))],
            )),
        );
        view
    }

    fn device_section_ids(view: &UiStudioView) -> Vec<&str> {
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane should exist");
        let UiViewContent::Stack(stack) = &device_pane.body else {
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

    fn project_read_response_with_runtime(id: u64, revision: Revision) -> WireServerMessage {
        WireServerMessage {
            id,
            msg: WireServerMsgBody::ProjectReadFrame {
                frame: ProjectReadFrame::new(
                    0,
                    vec![
                        ProjectReadEvent::Begin { revision },
                        ProjectReadEvent::Query {
                            index: 0,
                            event: ProjectReadQueryEvent::Runtime(RuntimeReadResult {
                                project: ProjectRuntimeStatus {
                                    revision,
                                    frame_num: 77,
                                    frame_delta_ms: 16,
                                    frame_total_ms: 17,
                                    demand_root_count: 2,
                                    runtime_buffer_count: 3,
                                },
                                server: Some(ServerRuntimeStatus {
                                    theoretical_fps: Some(60.0),
                                    last_frame_time_us: Some(16_000),
                                    memory: Some(MemoryStats {
                                        free_bytes: 4096,
                                        used_bytes: 2048,
                                        total_bytes: 6144,
                                    }),
                                }),
                            }),
                        },
                        ProjectReadEvent::End { revision },
                    ],
                ),
            },
        }
    }

    struct ScriptedClientIo {
        sent: Rc<RefCell<Vec<ClientMessage>>>,
        responses: Rc<RefCell<VecDeque<WireServerMessage>>>,
    }

    impl ScriptedClientIo {
        fn new(sent: Rc<RefCell<Vec<ClientMessage>>>, responses: Vec<WireServerMessage>) -> Self {
            Self {
                sent,
                responses: Rc::new(RefCell::new(responses.into())),
            }
        }
    }

    impl ClientIo for ScriptedClientIo {
        fn send<'life0, 'async_trait>(
            &'life0 mut self,
            msg: ClientMessage,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            self.sent.borrow_mut().push(msg);
            Box::pin(async { Ok(()) })
        }

        fn receive<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            let response =
                self.responses.borrow_mut().pop_front().ok_or_else(|| {
                    TransportError::Other("scripted client io exhausted".to_string())
                });
            Box::pin(async move { response })
        }

        fn close<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(()) })
        }
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
