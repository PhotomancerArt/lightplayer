use crate::UxLogEntry;

pub struct ProjectSyncRun {
    pub logs: Vec<UxLogEntry>,
    pub synced: bool,
}

impl ProjectSyncRun {
    pub fn synced(logs: Vec<UxLogEntry>) -> Self {
        Self { logs, synced: true }
    }

    pub fn failed(logs: Vec<UxLogEntry>) -> Self {
        Self {
            logs,
            synced: false,
        }
    }
}
