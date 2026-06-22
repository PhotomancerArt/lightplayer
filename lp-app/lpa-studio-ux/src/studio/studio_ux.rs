use crate::{
    AvailableAction, ConnectedLink, LinkAction, LinkOpenOutcome, LinkUx, ProjectAction, ProjectUx,
    ServerUx, StudioAction, StudioSnapshot, UxLogEntry, UxLogLevel, UxNotice, UxOutcome, UxResult,
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

    pub fn actions(&self) -> Vec<AvailableAction<StudioAction>> {
        let mut actions = self
            .link
            .actions()
            .into_iter()
            .map(|action| action.map(StudioAction::from))
            .collect::<Vec<_>>();
        actions.extend(
            self.project
                .actions(self.server.is_connected())
                .into_iter()
                .map(|action| action.map(StudioAction::from)),
        );
        actions
    }

    pub async fn execute(&mut self, action: StudioAction) -> UxResult {
        match action {
            StudioAction::Link(action) => self.execute_link_action(action).await,
            StudioAction::Project(ProjectAction::LoadDemoProject) => self.load_demo_project().await,
        }
    }

    async fn execute_link_action(&mut self, action: LinkAction) -> UxResult {
        match action {
            LinkAction::RefreshProviders => {
                self.link.refresh_provider_catalog();
                Ok(UxOutcome::new().with_notice(UxNotice::info("Provider catalog refreshed")))
            }
            LinkAction::OpenProvider { provider_id } => {
                match self.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UxOutcome::new()),
                    LinkOpenOutcome::Connected(connected) => self.attach_connected_link(connected),
                }
            }
            LinkAction::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                let connected = self.link.connect_endpoint(provider_id, endpoint_id).await?;
                self.attach_connected_link(connected)
            }
        }
    }

    fn attach_connected_link(&mut self, connected: ConnectedLink) -> UxResult {
        self.logs.extend(connected.logs);
        match self
            .server
            .attach_link_connection(self.link.registry_handle(), &connected.connection)
        {
            Ok(()) => Ok(UxOutcome::new().with_notice(UxNotice::info("Server protocol connected"))),
            Err(error) => {
                self.server.fail(error.to_string());
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
}

impl Default for StudioUx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{LinkState, StudioAction};

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
    fn initial_actions_are_link_actions() {
        let studio = StudioUx::new();

        let actions = studio.actions();

        assert!(
            actions
                .iter()
                .all(|action| matches!(action.command, StudioAction::Link(_)))
        );
    }
}
