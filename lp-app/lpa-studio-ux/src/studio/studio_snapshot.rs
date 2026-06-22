use crate::{LinkSnapshot, ProjectSnapshot, ServerSnapshot, UxLogEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StudioSnapshot {
    pub link: LinkSnapshot,
    pub server: ServerSnapshot,
    pub project: ProjectSnapshot,
    pub logs: Vec<UxLogEntry>,
}

impl StudioSnapshot {
    pub fn new(
        link: LinkSnapshot,
        server: ServerSnapshot,
        project: ProjectSnapshot,
        logs: Vec<UxLogEntry>,
    ) -> Self {
        Self {
            link,
            server,
            project,
            logs,
        }
    }
}
