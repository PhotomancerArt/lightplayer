use serde::{Deserialize, Serialize};

use crate::ActionId;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StudioDiagnostic {
    pub action_id: Option<ActionId>,
    pub severity: StudioDiagnosticSeverity,
    pub message: String,
}

impl StudioDiagnostic {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            action_id: None,
            severity: StudioDiagnosticSeverity::Info,
            message: message.into(),
        }
    }

    pub fn error(action_id: Option<ActionId>, message: impl Into<String>) -> Self {
        Self {
            action_id,
            severity: StudioDiagnosticSeverity::Error,
            message: message.into(),
        }
    }
}
