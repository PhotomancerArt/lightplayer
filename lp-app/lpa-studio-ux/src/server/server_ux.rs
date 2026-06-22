use crate::{ProgressState, ServerSnapshot, ServerState, UxIssue};

pub struct ServerUx {
    state: ServerState,
}

impl ServerUx {
    pub fn new() -> Self {
        Self {
            state: ServerState::Disconnected,
        }
    }

    pub fn set_state(&mut self, state: ServerState) {
        self.state = state;
    }

    pub fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot::new(self.state.clone())
    }

    pub fn mark_connecting(&mut self, label: impl Into<String>) {
        self.state = ServerState::Connecting {
            progress: ProgressState::new(label),
        };
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = ServerState::Failed {
            issue: UxIssue::new(message),
        };
    }
}

impl Default for ServerUx {
    fn default() -> Self {
        Self::new()
    }
}
