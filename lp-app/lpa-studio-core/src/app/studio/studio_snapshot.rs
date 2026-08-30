use crate::{ProjectSnapshot, ServerSnapshot, UiLogEntry};

#[derive(Clone, Debug, PartialEq)]
pub struct StudioSnapshot {
    pub server: ServerSnapshot,
    pub project: ProjectSnapshot,
    pub logs: Vec<UiLogEntry>,
}

impl StudioSnapshot {
    pub fn new(server: ServerSnapshot, project: ProjectSnapshot, logs: Vec<UiLogEntry>) -> Self {
        Self {
            server,
            project,
            logs,
        }
    }
}
