use core::future::Future;

use lpa_link::{LinkConnection, LinkConnectionKind};

use crate::{
    ConnectedLink, LinkOp, LinkOpenOutcome, LinkUx, ProjectConnectResult, ProjectOp, ProjectUx,
    ServerOp, ServerUx, StudioSnapshot, StudioView, UxAction, UxActions, UxContext, UxError,
    UxLogEntry, UxLogLevel, UxNode, UxNotice, UxOutcome, UxResult,
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
        if action.node_id() == &self.link.node_id() {
            let op = action.into_op::<LinkOp>()?;
            return self.execute_link_op(op).await;
        }
        if action.node_id() == &self.project.node_id() {
            let op = action.into_op::<ProjectOp>()?;
            return self.execute_project_op(op).await;
        }
        if action.node_id() == &self.server.node_id() {
            let op = action.into_op::<ServerOp>()?;
            return self.execute_server_op(op).await;
        }
        Err(crate::UxError::UnsupportedAction(format!(
            "unknown UX node {}",
            action.node_id()
        )))
    }

    async fn execute_link_op(&mut self, op: LinkOp) -> UxResult {
        match op {
            LinkOp::DisconnectLink => self.disconnect_link().await,
            LinkOp::ConnectServer => self.connect_server_from_link().await,
            LinkOp::RefreshProviders => {
                self.link.refresh_provider_catalog();
                Ok(UxOutcome::new().with_notice(UxNotice::info("Provider catalog refreshed")))
            }
            LinkOp::OpenProvider { provider_id } => {
                match self.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UxOutcome::new()),
                    LinkOpenOutcome::Connected(connected) => {
                        self.attach_connected_link(connected).await
                    }
                }
            }
            LinkOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                let connected = self.link.connect_endpoint(provider_id, endpoint_id).await?;
                self.attach_connected_link(connected).await
            }
        }
    }

    async fn execute_project_op(&mut self, op: ProjectOp) -> UxResult {
        match op {
            ProjectOp::ConnectRunningProject => self.connect_running_project().await,
            ProjectOp::ConnectLoadedProject { handle_id } => {
                self.connect_loaded_project(handle_id).await
            }
            ProjectOp::LoadDemoProject => self.load_demo_project().await,
            ProjectOp::DisconnectProject => self.disconnect_project().await,
        }
    }

    async fn execute_server_op(&mut self, op: ServerOp) -> UxResult {
        match op {
            ServerOp::DisconnectServer => self.disconnect_server().await,
        }
    }

    async fn attach_connected_link(&mut self, connected: ConnectedLink) -> UxResult {
        self.logs.extend(connected.logs);
        self.connect_server_connection(&connected.connection).await
    }

    async fn connect_server_from_link(&mut self) -> UxResult {
        let connection = self
            .link
            .active_connection()
            .ok_or_else(|| UxError::MissingSession("link connection is not open".to_string()))?;
        self.connect_server_connection(&connection).await
    }

    async fn connect_server_connection(&mut self, connection: &LinkConnection) -> UxResult {
        match self
            .server
            .attach_link_connection(self.link.registry_handle(), connection)
        {
            Ok(()) => {
                let mut outcome =
                    UxOutcome::new().with_notice(UxNotice::info("Server protocol connected"));
                match self.connect_running_project_if_available().await? {
                    AutoProjectConnect::Connected => {
                        outcome = outcome.with_notice(UxNotice::info("Connected running project"));
                    }
                    AutoProjectConnect::SelectionRequired => {
                        outcome = outcome.with_notice(UxNotice::info("Choose running project"));
                    }
                    AutoProjectConnect::NotFound if should_auto_load_demo_project(connection) => {
                        let demo_outcome = self.load_demo_project().await?;
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

    async fn connect_running_project(&mut self) -> UxResult {
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
    ) -> Result<AutoProjectConnect, UxError> {
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

    async fn connect_loaded_project(&mut self, handle_id: u32) -> UxResult {
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

    async fn load_demo_project(&mut self) -> UxResult {
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

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use lpa_link::{LinkConnection, LinkProviderKind};

    use crate::{
        ConnectedDeviceSummary, LinkState, LinkUx, ProjectInventorySummary, ProjectState,
        ServerState,
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
