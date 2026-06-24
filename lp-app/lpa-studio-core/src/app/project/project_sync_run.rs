use crate::UiLogEntry;

pub struct ProjectSyncRun {
    pub logs: Vec<UiLogEntry>,
    pub synced: bool,
}

impl ProjectSyncRun {
    pub fn synced(logs: Vec<UiLogEntry>) -> Self {
        Self { logs, synced: true }
    }

    pub fn failed(logs: Vec<UiLogEntry>) -> Self {
        Self {
            logs,
            synced: false,
        }
    }
}
