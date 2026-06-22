use crate::{ActionKind, UxCommand};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkAction {
    StartSimulator,
    RetrySimulator,
}

impl LinkAction {
    pub const START_SIMULATOR: ActionKind = ActionKind::new("link", "start-simulator");
    pub const RETRY_SIMULATOR: ActionKind = ActionKind::new("link", "retry-simulator");
}

impl UxCommand for LinkAction {
    fn action_kind(&self) -> ActionKind {
        match self {
            Self::StartSimulator => Self::START_SIMULATOR,
            Self::RetrySimulator => Self::RETRY_SIMULATOR,
        }
    }
}
