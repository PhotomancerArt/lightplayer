use crate::{
    AvailableAction, LinkAction, LinkUx, ProjectAction, ProjectUx, ServerState, ServerUx,
    StudioAction, StudioSnapshot, UxError, UxLogEntry, UxResult,
};
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
use crate::{LinkState, ProjectState, UxLogLevel, UxNotice, UxOutcome};

pub struct StudioUx {
    link: LinkUx,
    server: ServerUx,
    project: ProjectUx,
    logs: Vec<UxLogEntry>,
    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    browser_worker: Option<crate::browser_worker::BrowserWorkerRuntime>,
}

impl StudioUx {
    pub fn new() -> Self {
        Self {
            link: LinkUx::new(),
            server: ServerUx::new(),
            project: ProjectUx::new(),
            logs: Vec::new(),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            browser_worker: Some(crate::browser_worker::BrowserWorkerRuntime::new()),
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
        let server_connected =
            matches!(self.server.snapshot().state, ServerState::Connected { .. });
        actions.extend(
            self.project
                .actions(server_connected)
                .into_iter()
                .map(|action| action.map(StudioAction::from)),
        );
        actions
    }

    pub async fn execute(&mut self, action: StudioAction) -> UxResult {
        match action {
            StudioAction::Link(LinkAction::StartSimulator | LinkAction::RetrySimulator) => {
                self.start_simulator().await
            }
            StudioAction::Project(ProjectAction::LoadDemoProject) => self.load_demo_project().await,
        }
    }

    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    async fn start_simulator(&mut self) -> UxResult {
        self.link.mark_starting("Starting simulator");
        self.server.mark_connecting("Opening server protocol");
        self.project.set_state(ProjectState::NotLoaded);

        let runtime = self
            .browser_worker
            .as_mut()
            .ok_or_else(|| UxError::MissingSession("browser worker runtime is missing".into()))?;
        match runtime.start().await {
            Ok(started) => {
                self.logs.extend(started.logs);
                self.link.set_state(LinkState::Connected {
                    device: started.device,
                });
                self.server.set_state(ServerState::Connected {
                    protocol: started.protocol,
                });
                Ok(UxOutcome::new().with_notice(UxNotice::info("Simulator is running")))
            }
            Err(error) => {
                self.link.fail(error.to_string());
                self.server.fail(error.to_string());
                Err(error)
            }
        }
    }

    #[cfg(not(all(feature = "browser-worker", target_arch = "wasm32")))]
    async fn start_simulator(&mut self) -> UxResult {
        let message = "browser-worker simulator is only available in wasm builds".to_string();
        self.link.fail(message.clone());
        Err(UxError::UnsupportedFeature(message))
    }

    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    async fn load_demo_project(&mut self) -> UxResult {
        if !matches!(self.server.snapshot().state, ServerState::Connected { .. }) {
            return Err(UxError::MissingSession(
                "start the simulator before loading a project".into(),
            ));
        }
        self.project.mark_loading_demo();

        let runtime = self
            .browser_worker
            .as_mut()
            .ok_or_else(|| UxError::MissingSession("browser worker runtime is missing".into()))?;
        match runtime.load_demo_project().await {
            Ok(loaded) => {
                self.logs.extend(loaded.logs);
                self.project
                    .mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
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

    #[cfg(not(all(feature = "browser-worker", target_arch = "wasm32")))]
    async fn load_demo_project(&mut self) -> UxResult {
        Err(UxError::UnsupportedFeature(
            "demo project loading requires the browser-worker feature on wasm".into(),
        ))
    }
}

impl Default for StudioUx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::LinkState;

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
    fn initial_actions_offer_start_simulator() {
        let studio = StudioUx::new();

        let actions = studio.actions();

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].command,
            StudioAction::from(LinkAction::StartSimulator)
        );
    }
}
