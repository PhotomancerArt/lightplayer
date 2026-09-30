use crate::UiLogDraft;

pub struct ProjectSyncRun {
    pub logs: Vec<UiLogDraft>,
    pub synced: bool,
    /// A failed run whose failure came FROM the board (a refused read, a
    /// reply that arrived malformed): the wire is alive even though the
    /// sync is not. The lens's dead-wire backstop counts only failures
    /// where nobody answered.
    pub(crate) board_answered: bool,
}

impl ProjectSyncRun {
    pub fn synced(logs: Vec<UiLogDraft>) -> Self {
        Self {
            logs,
            synced: true,
            board_answered: true,
        }
    }

    pub fn failed(logs: Vec<UiLogDraft>) -> Self {
        Self::failed_after(logs, false)
    }

    /// A failed run; `board_answered` as on [`Self::board_answered`].
    pub(crate) fn failed_after(logs: Vec<UiLogDraft>, board_answered: bool) -> Self {
        Self {
            logs,
            synced: false,
            board_answered,
        }
    }
}
