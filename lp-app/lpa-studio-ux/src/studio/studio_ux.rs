use core::future::Future;

use lpa_link::{LinkConnection, LinkConnectionKind, LinkManagementRequest, LinkManagementResult};

use crate::{
    ConnectedLink, LinkOp, LinkOpenOutcome, LinkUx, ProjectConnectResult, ProjectOp, ProjectUx,
    ServerOp, ServerUx, StudioSnapshot, StudioView, UxAction, UxActions, UxActivity, UxContext,
    UxError, UxLogEntry, UxLogLevel, UxNode, UxNotice, UxOutcome, UxProgress, UxResult, UxStatus,
    UxUpdate, UxUpdateSink,
};

pub struct StudioUx {
    link: LinkUx,
    server: ServerUx,
    project: ProjectUx,
    logs: Vec<UxLogEntry>,
}

impl StudioUx {
    pub fn new() -> Self {
        Self {
            link: LinkUx::new(),
            server: ServerUx::new(),
            project: ProjectUx::new(),
            logs: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> StudioSnapshot {
        StudioSnapshot::new(
            self.link.snapshot(),
            self.server.snapshot(),
            self.project.snapshot(),
            self.logs.clone(),
        )
    }

    pub fn actions(&self) -> UxActions {
        let mut actions = UxActions::new(self.link.actions(self.server.is_connected()));
        actions.extend(self.server.actions());
        actions.extend(self.project.actions(self.server.is_connected()));
        actions
    }

    pub fn view(&self) -> StudioView {
        StudioView::new(
            vec![
                self.link.view(self.server.is_connected()),
                self.server.view(),
                self.project.view(self.server.is_connected()),
            ],
            self.logs.clone(),
        )
    }

    pub async fn dispatch(&mut self, action: UxAction) -> UxResult {
        self.dispatch_with_updates(action, UxUpdateSink::noop())
            .await
    }

    pub async fn dispatch_with_updates(
        &mut self,
        action: UxAction,
        updates: UxUpdateSink,
    ) -> UxResult {
        updates.emit(UxUpdate::View(self.view()));
        let result = self.dispatch_inner(action, updates.clone()).await;
        updates.emit(UxUpdate::View(self.view()));
        result
    }

    async fn dispatch_inner(&mut self, action: UxAction, updates: UxUpdateSink) -> UxResult {
        if action.node_id() == &self.link.node_id() {
            let op = action.into_op::<LinkOp>()?;
            return self.execute_link_op(op, updates).await;
        }
        if action.node_id() == &self.project.node_id() {
            let op = action.into_op::<ProjectOp>()?;
            return self.execute_project_op(op, updates).await;
        }
        if action.node_id() == &self.server.node_id() {
            let op = action.into_op::<ServerOp>()?;
            return self.execute_server_op(op, updates).await;
        }
        Err(crate::UxError::UnsupportedAction(format!(
            "unknown UX node {}",
            action.node_id()
        )))
    }

    async fn execute_link_op(&mut self, op: LinkOp, updates: UxUpdateSink) -> UxResult {
        match op {
            LinkOp::DisconnectLink => self.disconnect_link().await,
            LinkOp::ConnectServer => self.connect_server_from_link(updates).await,
            LinkOp::ProvisionFirmware => self.provision_firmware(updates).await,
            LinkOp::ResetToBlank => self.reset_to_blank(updates).await,
            LinkOp::RefreshProviders => {
                self.link.refresh_provider_catalog();
                Ok(UxOutcome::new().with_notice(UxNotice::info("Provider catalog refreshed")))
            }
            LinkOp::OpenProvider { provider_id } => {
                emit_activity(
                    &updates,
                    self.link.node_id(),
                    "Opening link",
                    "Opening",
                    UxProgress::indeterminate(format!("Opening {}", provider_id.label())),
                );
                match self.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UxOutcome::new()),
                    LinkOpenOutcome::Connected(connected) => {
                        self.attach_connected_link(connected, updates).await
                    }
                }
            }
            LinkOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                emit_activity(
                    &updates,
                    self.link.node_id(),
                    "Opening link session",
                    "Connecting",
                    UxProgress::indeterminate("Opening link endpoint"),
                );
                let connected = self.link.connect_endpoint(provider_id, endpoint_id).await?;
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

    async fn execute_server_op(&mut self, op: ServerOp, _updates: UxUpdateSink) -> UxResult {
        match op {
            ServerOp::DisconnectServer => self.disconnect_server().await,
        }
    }

    async fn attach_connected_link(
        &mut self,
        connected: ConnectedLink,
        updates: UxUpdateSink,
    ) -> UxResult {
        self.logs.extend(connected.logs);
        self.connect_server_connection(&connected.connection, updates)
            .await
    }

    async fn connect_server_from_link(&mut self, updates: UxUpdateSink) -> UxResult {
        let connection = self
            .link
            .active_connection()
            .ok_or_else(|| UxError::MissingSession("link connection is not open".to_string()))?;
        if should_reopen_before_server_connect(&connection) {
            self.project.reset();
            self.server.disconnect();
            emit_activity(
                &updates,
                self.link.node_id(),
                "Reopening link",
                "Connecting",
                UxProgress::indeterminate("Resetting device before server connect"),
            );
            let connected = self.link.reopen_active_connection().await?;
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
            self.server.node_id(),
            "Connecting server",
            "Connecting",
            UxProgress::indeterminate("Opening server protocol"),
        );
        match self.server.attach_link_connection(
            self.link.registry_handle(),
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
                    UxProgress::timeout("Waiting for server response", 5_000),
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
                        self.logs.extend(self.server.take_pending_logs());
                        self.project.reset();
                        if matches!(error, UxError::NoFirmwareDetected(_)) {
                            self.server.fail("No LightPlayer firmware detected.");
                            return Ok(UxOutcome::new().with_notice(UxNotice::info(
                                "No LightPlayer firmware detected; provision the selected ESP32",
                            )));
                        }
                        self.server.fail(error.to_string());
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
                self.server.fail(error.to_string());
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
            UxProgress::timeout("Waiting for loaded projects", 5_000),
        );
        let result = {
            let server = self.server.client_mut()?;
            self.project.connect_running_project(server).await
        };
        match result {
            Ok(ProjectConnectResult::Connected { logs }) => {
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Connected running project")))
            }
            Ok(ProjectConnectResult::SelectionRequired { logs }) => {
                self.logs.extend(logs);
                Ok(UxOutcome::new().with_notice(UxNotice::info("Choose running project")))
            }
            Ok(ProjectConnectResult::NotFound { logs }) => {
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
            UxProgress::timeout("Waiting for loaded projects", 5_000),
        );
        let result = {
            let server = self.server.client_mut()?;
            self.project
                .connect_running_project_if_available(server)
                .await
        };
        match result? {
            ProjectConnectResult::Connected { logs } => {
                self.logs.extend(logs);
                Ok(AutoProjectConnect::Connected)
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

    async fn connect_loaded_project(&mut self, handle_id: u32, updates: UxUpdateSink) -> UxResult {
        emit_activity(
            &updates,
            self.project.node_id(),
            "Connecting project",
            "Connecting",
            UxProgress::indeterminate("Loading project shape"),
        );
        let result = {
            let server = self.server.client_mut()?;
            self.project.connect_loaded_project(server, handle_id).await
        };
        match result {
            Ok(logs) => {
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
            UxProgress::indeterminate("Uploading demo project"),
        );
        let result = {
            let server = self.server.client_mut()?;
            self.project.load_demo_project(server).await
        };
        match result {
            Ok(logs) => {
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

    async fn disconnect_server(&mut self) -> UxResult {
        self.project.reset();
        self.server.disconnect();
        Ok(UxOutcome::new().with_notice(UxNotice::info("Server disconnected")))
    }

    async fn disconnect_link(&mut self) -> UxResult {
        self.project.reset();
        self.server.disconnect();
        self.link.disconnect().await?;
        Ok(UxOutcome::new().with_notice(UxNotice::info("Link disconnected")))
    }

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.server.disconnect();
        let management = self
            .link
            .manage_with_updates(
                LinkManagementRequest::FlashFirmware,
                "Provisioning firmware",
                updates.clone(),
            )
            .await?;
        self.logs.extend(management.logs);
        let mut outcome = UxOutcome::new().with_notice(provision_notice(&management.result));
        emit_activity(
            &updates,
            self.link.node_id(),
            "Reconnecting device",
            "Connecting",
            UxProgress::timeout("Waiting for firmware boot", 5_000),
        );
        match self.link.reopen_active_connection().await {
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
                    self.server.fail(error.to_string());
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
                self.server.fail(error.to_string());
                Ok(outcome.with_notice(UxNotice::info(
                    "Firmware provisioned; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UxResult {
        self.project.reset();
        self.server.disconnect();
        let management = self
            .link
            .manage_with_updates(
                LinkManagementRequest::EraseDeviceFlash,
                "Resetting device to blank",
                updates,
            )
            .await?;
        self.logs.extend(management.logs);
        Ok(UxOutcome::new().with_notice(reset_notice(&management.result)))
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
        action: UxAction,
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
    progress: UxProgress,
) {
    updates.emit(UxUpdate::Activity {
        node_id,
        status: UxStatus::working(status),
        activity: UxActivity::new(title).with_progress(progress),
    });
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
        ServerState, UxNodeId, UxStatusKind,
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
    fn initial_actions_target_link_node() {
        let studio = StudioUx::new();

        let actions = studio.actions();

        assert!(
            actions
                .iter()
                .all(|action| action.node_id().as_str() == LinkUx::NODE_ID)
        );
    }

    #[test]
    fn initial_view_exposes_three_panes() {
        let studio = StudioUx::new();

        let view = studio.view();

        assert_eq!(view.panes.len(), 3);
        assert_eq!(view.panes[0].node_id.as_str(), LinkUx::NODE_ID);
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
            studio.server.snapshot().state,
            ServerState::Connected { .. }
        ));
        assert!(matches!(
            studio.link.snapshot().state,
            LinkState::Connected { .. }
        ));
    }

    #[test]
    fn server_disconnect_clears_project_and_keeps_link_connected() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_server()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.server.snapshot().state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.link.snapshot().state,
            LinkState::Connected { .. }
        ));
    }

    #[test]
    fn server_disconnect_exposes_server_reconnect_action_on_link() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_server()).unwrap();
        let actions = studio.actions();

        assert!(actions.iter().any(|action| {
            action.node_id().as_str() == LinkUx::NODE_ID
                && action.op_as::<LinkOp>() == Some(&LinkOp::ConnectServer)
        }));
    }

    #[test]
    fn link_disconnect_clears_project_server_and_link() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_link()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.server.snapshot().state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.link.snapshot().state,
            LinkState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn failed_link_dispatch_emits_final_failed_view_after_activity() {
        let mut studio = StudioUx::new();
        studio.link = LinkUx::with_registry(registry_with_fake_connect_error(
            "Failed to open serial port.",
        ));
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| {
                updates.borrow_mut().push(update);
            }
        });
        let action = UxAction::from_op(
            UxNodeId::new(LinkUx::NODE_ID),
            LinkOp::ConnectEndpoint {
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
                    if activity.title == "Opening link session"
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
        assert_eq!(last_view.panes[0].status.kind, UxStatusKind::Error);
        assert_eq!(last_view.panes[0].status.label, "Link failed");
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
        studio.link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
        studio.server.set_state(ServerState::Connected {
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
