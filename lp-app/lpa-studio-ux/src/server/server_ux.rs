use lpa_link::LinkConnection;

use crate::{
    ProgressState, ServerSnapshot, ServerState, SharedLinkRegistry, StudioServerClient, UxError,
    UxIssue,
};

pub struct ServerUx {
    state: ServerState,
    client: Option<StudioServerClient>,
}

impl ServerUx {
    pub fn new() -> Self {
        Self {
            state: ServerState::Disconnected,
            client: None,
        }
    }

    pub fn set_state(&mut self, state: ServerState) {
        self.state = state;
    }

    pub fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot::new(self.state.clone())
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, ServerState::Connected { .. }) && self.client.is_some()
    }

    pub fn mark_connecting(&mut self, label: impl Into<String>) {
        self.state = ServerState::Connecting {
            progress: ProgressState::new(label),
        };
    }

    pub fn attach_link_connection(
        &mut self,
        registry: SharedLinkRegistry,
        connection: &LinkConnection,
    ) -> Result<(), UxError> {
        self.mark_connecting("Opening server protocol");
        let client = StudioServerClient::from_link_connection(registry, connection)?;
        let protocol = client.protocol().to_string();
        self.client = Some(client);
        self.state = ServerState::Connected { protocol };
        Ok(())
    }

    pub fn client_mut(&mut self) -> Result<&mut StudioServerClient, UxError> {
        self.client
            .as_mut()
            .ok_or_else(|| UxError::MissingSession("server client is not connected".to_string()))
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.client = None;
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
