use serde::{Deserialize, Serialize};

/// Scripted result of requesting access to a provider or user-selected device.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AccessOutcome {
    Granted,
    PermissionCanceled { reason: String },
    PermissionDenied { reason: String },
    Unsupported { reason: String },
}

impl AccessOutcome {
    pub fn permission_canceled(reason: impl Into<String>) -> Self {
        Self::PermissionCanceled {
            reason: reason.into(),
        }
    }

    pub fn permission_denied(reason: impl Into<String>) -> Self {
        Self::PermissionDenied {
            reason: reason.into(),
        }
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unsupported {
            reason: reason.into(),
        }
    }
}

impl Default for AccessOutcome {
    fn default() -> Self {
        Self::Granted
    }
}
