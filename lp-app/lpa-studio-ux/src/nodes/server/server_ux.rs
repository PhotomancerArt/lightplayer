use lpa_link::LinkConnection;

use crate::{
    ProgressState, ServerFailureKind, ServerOp, ServerSnapshot, ServerState, SharedLinkRegistry,
    StudioServerClient, UiAction, UiBody, UiMetric, UiPaneView, UiStatus, UxError, UxIssue, UxNode,
    UxNodeId, UxUpdateSink,
};

pub struct ServerUx {
    state: ServerState,
    client: Option<StudioServerClient>,
}

impl ServerUx {
    pub const NODE_ID: &'static str = "studio.server";

    pub fn new() -> Self {
        Self {
            state: ServerState::Disconnected,
            client: None,
        }
    }

    pub fn set_state(&mut self, state: ServerState) {
        self.state = state;
    }

    #[cfg(test)]
    pub(crate) fn set_client_for_test(&mut self, client: StudioServerClient) {
        let protocol = client.protocol().to_string();
        self.client = Some(client);
        self.state = ServerState::Connected { protocol };
    }

    pub fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot::new(self.state.clone())
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, ServerState::Connected { .. }) && self.client.is_some()
    }

    pub fn actions(&self) -> Vec<UiAction> {
        match self.state {
            ServerState::Connected { .. } => vec![self.action(ServerOp::DisconnectServer)],
            ServerState::Disconnected
            | ServerState::Connecting { .. }
            | ServerState::Failed { .. } => Vec::new(),
        }
    }

    pub fn view(&self) -> UiPaneView {
        UiPaneView::new(
            Self::NODE_ID,
            "Server",
            server_status(&self.state),
            server_body(&self.state),
            self.actions(),
        )
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
        updates: UxUpdateSink,
    ) -> Result<(), UxError> {
        self.mark_connecting("Opening server protocol");
        let client = StudioServerClient::from_link_connection(registry, connection, updates)?;
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

    pub fn take_pending_logs(&mut self) -> Vec<crate::UxLogEntry> {
        self.client
            .as_mut()
            .map(StudioServerClient::take_pending_logs)
            .unwrap_or_default()
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.fail_with_kind(message, ServerFailureKind::Unknown);
    }

    pub fn fail_no_firmware(&mut self) {
        self.fail_with_kind(
            "No LightPlayer firmware detected.",
            ServerFailureKind::NoFirmware,
        );
    }

    pub fn fail_with_kind(&mut self, message: impl Into<String>, kind: ServerFailureKind) {
        self.client = None;
        self.state = ServerState::Failed {
            issue: UxIssue::new(message),
            kind,
        };
    }

    pub fn disconnect(&mut self) {
        self.client = None;
        self.state = ServerState::Disconnected;
    }
}

impl UxNode for ServerUx {
    type Op = ServerOp;

    fn node_id(&self) -> UxNodeId {
        UxNodeId::new(Self::NODE_ID)
    }
}

impl Default for ServerUx {
    fn default() -> Self {
        Self::new()
    }
}

fn server_status(state: &ServerState) -> UiStatus {
    match state {
        ServerState::Disconnected => UiStatus::neutral("Offline"),
        ServerState::Connecting { .. } => UiStatus::working("Connecting"),
        ServerState::Connected { .. } => UiStatus::good("Connected"),
        ServerState::Failed { .. } => UiStatus::error("Failed"),
    }
}

fn server_body(state: &ServerState) -> UiBody {
    match state {
        ServerState::Disconnected => {
            UiBody::text("Open a link endpoint to attach the server protocol.")
        }
        ServerState::Connecting { progress } => UiBody::Progress(progress.clone()),
        ServerState::Connected { protocol } => {
            UiBody::Metrics(vec![UiMetric::new("Protocol", protocol)])
        }
        ServerState::Failed { issue, .. } => UiBody::Issue(issue.clone()),
    }
}
