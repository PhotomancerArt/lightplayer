use crate::{ProgressState, UxIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerState {
    Disconnected,
    Connecting { progress: ProgressState },
    Connected { protocol: String },
    Failed { issue: UxIssue },
}
