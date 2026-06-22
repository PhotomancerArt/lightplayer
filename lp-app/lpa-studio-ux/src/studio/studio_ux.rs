use core::future::Future;

use crate::{
    ConnectedLink, LinkOp, LinkOpenOutcome, LinkUx, ProjectOp, ProjectUx, ServerOp, ServerUx,
    StudioSnapshot, UxAction, UxContext, UxLogEntry, UxLogLevel, UxNode, UxNotice, UxOutcome,
    UxResult,
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

    pub fn actions(&self) -> Vec<UxAction> {
        let mut actions = self.link.actions();
        actions.extend(self.server.actions());
        actions.extend(self.project.actions(self.server.is_connected()));
        actions
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
        match self
            .server
            .attach_link_connection(self.link.registry_handle(), &connected.connection)
        {
            Ok(()) => {
                let mut outcome =
                    UxOutcome::new().with_notice(UxNotice::info("Server protocol connected"));
                if self.connect_running_project_if_available().await? {
                    outcome = outcome.with_notice(UxNotice::info("Connected running project"));
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
                Err(error)
            }
        }
    }

    async fn connect_running_project_if_available(&mut self) -> Result<bool, crate::UxError> {
        let result = {
            let server = self.server.client_mut()?;
            self.project
                .connect_running_project_if_available(server)
                .await
        };
        if let Some(logs) = result? {
            self.logs.extend(logs);
            return Ok(true);
        }
        Ok(false)
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
        self.project.disconnect();
        self.server.disconnect();
        Ok(UxOutcome::new().with_notice(UxNotice::info("Server disconnected")))
    }

    async fn disconnect_link(&mut self) -> UxResult {
        self.project.disconnect();
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

#[cfg(test)]
mod tests {
    use crate::{LinkState, LinkUx};

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
}
