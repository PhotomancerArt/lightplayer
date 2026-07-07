use crate::{LinkSnapshot, ProjectSnapshot, ServerSnapshot, UiLogEntry};

#[derive(Clone, Debug, PartialEq)]
pub struct StudioSnapshot {
    pub link: LinkSnapshot,
    pub server: ServerSnapshot,
    pub project: ProjectSnapshot,
    pub logs: Vec<UiLogEntry>,
}

impl StudioSnapshot {
    pub fn new(
        link: LinkSnapshot,
        server: ServerSnapshot,
        project: ProjectSnapshot,
        logs: Vec<UiLogEntry>,
    ) -> Self {
        Self {
            link,
            server,
            project,
            logs,
        }
    }
}
