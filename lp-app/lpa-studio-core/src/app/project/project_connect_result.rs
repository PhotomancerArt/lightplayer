use crate::UxLogEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectConnectResult {
    Connected { logs: Vec<UxLogEntry> },
    SelectionRequired { logs: Vec<UxLogEntry> },
    NotFound { logs: Vec<UxLogEntry> },
}

impl ProjectConnectResult {
    pub fn logs(self) -> Vec<UxLogEntry> {
        match self {
            Self::Connected { logs }
            | Self::SelectionRequired { logs }
            | Self::NotFound { logs } => logs,
        }
    }
}
