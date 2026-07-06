use crate::UiLogDraft;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectConnectResult {
    Connected { logs: Vec<UiLogDraft> },
    SelectionRequired { logs: Vec<UiLogDraft> },
    NotFound { logs: Vec<UiLogDraft> },
}

impl ProjectConnectResult {
    pub fn logs(self) -> Vec<UiLogDraft> {
        match self {
            Self::Connected { logs }
            | Self::SelectionRequired { logs }
            | Self::NotFound { logs } => logs,
        }
    }
}
