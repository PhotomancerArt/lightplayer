use crate::UiLogEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectConnectResult {
    Connected { logs: Vec<UiLogEntry> },
    SelectionRequired { logs: Vec<UiLogEntry> },
    NotFound { logs: Vec<UiLogEntry> },
}

impl ProjectConnectResult {
    pub fn logs(self) -> Vec<UiLogEntry> {
        match self {
            Self::Connected { logs }
            | Self::SelectionRequired { logs }
            | Self::NotFound { logs } => logs,
        }
    }
}
