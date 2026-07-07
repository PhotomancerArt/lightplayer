use crate::UiLogDraft;

pub struct ProjectSyncRun {
    pub logs: Vec<UiLogDraft>,
    pub synced: bool,
}

impl ProjectSyncRun {
    pub fn synced(logs: Vec<UiLogDraft>) -> Self {
        Self { logs, synced: true }
    }

    pub fn failed(logs: Vec<UiLogDraft>) -> Self {
        Self {
            logs,
            synced: false,
        }
    }
}
