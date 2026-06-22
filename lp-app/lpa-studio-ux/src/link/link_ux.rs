use crate::{
    ActionMeta, ActionPriority, AvailableAction, LinkAction, LinkSnapshot, LinkState,
    ProgressState, ProviderChoice, UxIssue,
};

pub struct LinkUx {
    state: LinkState,
}

impl LinkUx {
    pub fn new() -> Self {
        Self {
            state: LinkState::SelectingProvider {
                providers: vec![ProviderChoice::browser_worker()],
            },
        }
    }

    pub fn state(&self) -> &LinkState {
        &self.state
    }

    pub fn set_state(&mut self, state: LinkState) {
        self.state = state;
    }

    pub fn snapshot(&self) -> LinkSnapshot {
        LinkSnapshot::new(self.state.clone())
    }

    pub fn actions(&self) -> Vec<AvailableAction<LinkAction>> {
        match &self.state {
            LinkState::SelectingProvider { .. } => vec![AvailableAction::from_command(
                LinkAction::StartSimulator,
                ActionMeta::new(
                    LinkAction::START_SIMULATOR,
                    "Start simulator",
                    "Launch a browser-local LightPlayer runtime.",
                    ActionPriority::Primary,
                ),
            )],
            LinkState::Failed { .. } => vec![AvailableAction::from_command(
                LinkAction::RetrySimulator,
                ActionMeta::new(
                    LinkAction::RETRY_SIMULATOR,
                    "Retry simulator",
                    "Try launching the browser-local runtime again.",
                    ActionPriority::Primary,
                ),
            )],
            LinkState::StartingSimulator { .. } | LinkState::Connected { .. } => Vec::new(),
        }
    }

    pub fn mark_starting(&mut self, label: impl Into<String>) {
        self.state = LinkState::StartingSimulator {
            progress: ProgressState::new(label),
        };
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = LinkState::Failed {
            issue: UxIssue::new(message),
        };
    }
}

impl Default for LinkUx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_provider_offers_start_simulator() {
        let link = LinkUx::new();

        let actions = link.actions();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].command, LinkAction::StartSimulator);
        assert_eq!(actions[0].meta.label, "Start simulator");
    }
}
