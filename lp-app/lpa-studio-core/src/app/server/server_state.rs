use crate::{ProgressState, UiIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerState {
    Disconnected,
    Connecting {
        progress: ProgressState,
    },
    Connected {
        protocol: String,
    },
    Failed {
        issue: UiIssue,
        kind: ServerFailureKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerFailureKind {
    /// The sim worker's wasm instance is condemned (a panic escaped a
    /// panic=abort export — poisoned-instance defect). The issue message
    /// carries the primary panic; recovery is a worker reboot, which the
    /// studio runs once per crash under a flap guard.
    SimCrashed,
    Unknown,
}
