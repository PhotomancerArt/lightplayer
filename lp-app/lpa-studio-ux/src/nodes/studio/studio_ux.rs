use core::future::Future;

use lpa_link::{LinkConnection, LinkConnectionKind, LinkManagementRequest, LinkManagementResult};

use crate::{
    ConnectedLink, DeviceOp, DeviceUx, LinkOpenOutcome, ProjectConnectResult, ProjectOp,
    ProjectState, ProjectUx, StudioSnapshot, StudioView, UiAction, UiActions, UiActivity, UiBody,
    UiProgress, UiStatus, UxContext, UxError, UxLogEntry, UxLogLevel, UxNode, UxNotice, UxOutcome,
    UxResult, UxUpdate, UxUpdateSink,
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
        let mut panes = vec![self.device.view(&project_snapshot.state, project_actions)];
        if self.project_is_loaded() {
            panes.push(self.project.view(self.device.has_lightplayer_state()));
        }
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
        if action.node_id() == &self.device.node_id() {
            let op = action.into_op::<DeviceOp>()?;
            return self.execute_device_op(op, updates).await;
        }
        if action.node_id() == &self.project.node_id() {
            let op = action.into_op::<ProjectOp>()?;
            return self.execute_project_op(op, updates).await;
        }
        Err(crate::UxError::UnsupportedAction(format!(
            "unknown UX node {}",
            action.node_id()
        )))
    }

    async fn execute_device_op(&mut self, op: DeviceOp, updates: UxUpdateSink) -> UxResult {
        match op {
            DeviceOp::DisconnectDevice => self.disconnect_device().await,
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
                emit_activity(
                    &updates,
                    self.device.node_id(),
                    "Opening device",
                    "Opening",
                    UiProgress::indeterminate(format!("Opening {}", provider_id.label())),
                );
                match self.device.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UxOutcome::new()),
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
                    self.device.node_id(),
                    "Opening device session",
                    "Connecting",
                    UiProgress::indeterminate("Opening device endpoint"),
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
                self.device.node_id(),
                "Reopening device",
                "Connecting",
                UiProgress::indeterminate("Resetting device before server connect"),
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
            self.device.node_id(),
            "Connecting LightPlayer",
            "Connecting",
            UiProgress::indeterminate("Opening server protocol"),
        );
        match self.device.server.attach_link_connection(
            self.device.link.registry_handle(),
            connection,
            updates.clone(),
        ) {
            Ok(()) => {
                let mut outcome =
                    UxOutcome::new().with_notice(UxNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                emit_activity(
                    &updates,
                    self.project.node_id(),
                    "Checking running projects",
                    "Checking",
                    UiProgress::timeout("Waiting for server response", 5_000),
                );
                let auto_connect = match self
                    .connect_running_project_if_available(updates.clone())
                    .await
                {
                    Ok(auto_connect) => auto_connect,
                    Err(error) => {
                        self.logs.push(UxLogEntry::new(
                            UxLogLevel::Error,
                            "lpa-studio-ux",
                            format!("server readiness probe failed: {error}"),
                        ));
                        let pending_logs = self.device.server.take_pending_logs();
                        self.device.record_logs(&pending_logs);
                        self.logs.extend(pending_logs);
                        self.project.reset();
                        if matches!(error, UxError::NoFirmwareDetected(_)) {
                            self.device.server.fail("No LightPlayer firmware detected.");
                            return Ok(UxOutcome::new().with_notice(UxNotice::info(
                                "No LightPlayer firmware detected; provision the selected ESP32",
                            )));
                        }
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
            self.project.node_id(),
            "Connecting project",
            "Connecting",
            UiProgress::timeout("Waiting for loaded projects", 5_000),
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
            self.project.node_id(),
            "Checking running projects",
            "Checking",
            UiProgress::timeout("Waiting for loaded projects", 5_000),
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
            self.project.node_id(),
            "Connecting project",
            "Connecting",
            UiProgress::indeterminate("Loading project shape"),
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
            self.project.node_id(),
            "Loading demo project",
            "Loading",
            UiProgress::indeterminate("Uploading demo project"),
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

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        let management = self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::FlashFirmware,
                "Provisioning firmware",
                updates.clone(),
            )
            .await?;
        self.device.record_logs(&management.logs);
        self.logs.extend(management.logs);
        let mut outcome = UxOutcome::new().with_notice(provision_notice(&management.result));
        emit_activity(
            &updates,
            self.device.node_id(),
            "Reconnecting device",
            "Connecting",
            UiProgress::timeout("Waiting for firmware boot", 5_000),
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
                        format!("firmware provisioned but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UxNotice::info(
                        "Firmware provisioned; reconnect the server after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.logs.push(UxLogEntry::new(
                    UxLogLevel::Warn,
                    "lpa-studio-ux",
                    format!("firmware provisioned but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UxNotice::info(
                    "Firmware provisioned; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.device.server.disconnect();
        let management = self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::EraseDeviceFlash,
                "Resetting device to blank",
                updates,
            )
            .await?;
        self.device.record_logs(&management.logs);
        self.logs.extend(management.logs);
        Ok(UxOutcome::new().with_notice(reset_notice(&management.result)))
    }

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
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
    node_id: crate::UxNodeId,
    title: impl Into<String>,
    status: impl Into<String>,
    progress: UiProgress,
) {
    updates.emit(UxUpdate::Activity {
        node_id,
        status: UiStatus::working(status),
        activity: UiActivity::new(title).with_progress(progress),
    });
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
            UxNotice::info(format!("Provisioned {}", result.manifest.display_name))
        }
        _ => UxNotice::info("Firmware provisioned"),
    }
}

fn reset_notice(result: &LinkManagementResult) -> UxNotice {
    match result {
        LinkManagementResult::EraseDeviceFlash(result) => {
            let label = result.chip_name.as_deref().unwrap_or("selected ESP32");
            UxNotice::info(format!("{label} reset to blank"))
        }
        _ => UxNotice::info("Device reset to blank"),
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
    use lpa_link::{LinkConnection, LinkEndpoint, LinkEndpointId, LinkProviderKind};

    use crate::{
        ConnectedDeviceSummary, LinkState, LinkUx, ProjectInventorySummary, ProjectState,
        ProjectUx, ServerState, UiStatusKind, UxNodeId,
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
    }

    #[test]
    fn connected_without_project_keeps_project_actions_in_device_pane() {
        let mut studio = connected_studio();
        studio.project.reset();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceUx::NODE_ID);
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
    }

    #[test]
    fn loaded_project_gets_project_pane() {
        let studio = connected_studio();

        let view = studio.view();

        assert_eq!(view.panes.len(), 2);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceUx::NODE_ID);
        assert_eq!(view.panes[1].node_id.as_str(), ProjectUx::NODE_ID);
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
                UxUpdate::Activity { activity, .. }
                    if activity.title == "Opening device session"
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

    fn connected_studio() -> StudioUx {
        let mut studio = StudioUx::new();
        studio.device.link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
        studio.device.server.set_state(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
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
