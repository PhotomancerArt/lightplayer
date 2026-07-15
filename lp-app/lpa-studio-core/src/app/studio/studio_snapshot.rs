use crate::{ConnectFlowState, ProjectSnapshot, ServerSnapshot, UiLogEntry};

#[derive(Clone, Debug, PartialEq)]
pub struct StudioSnapshot {
    pub flow: ConnectFlowState,
    pub server: ServerSnapshot,
    pub project: ProjectSnapshot,
    pub logs: Vec<UiLogEntry>,
}

impl StudioSnapshot {
    pub fn new(
        flow: ConnectFlowState,
        server: ServerSnapshot,
        project: ProjectSnapshot,
        logs: Vec<UiLogEntry>,
    ) -> Self {
        Self {
            flow,
            server,
            project,
            logs,
        }
    }
}
