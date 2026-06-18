use serde::{Deserialize, Serialize};

use crate::RecoveryAction;

/// Whether a provider can currently be used in this Studio runtime.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProviderAvailability {
    Available,
    AvailableWithPermission,
    Unavailable {
        reason: String,
        recovery_actions: Vec<RecoveryAction>,
    },
    HiddenInThisBuild,
}

impl ProviderAvailability {
    pub fn unavailable(reason: impl Into<String>, recovery_actions: Vec<RecoveryAction>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            recovery_actions,
        }
    }

    pub fn can_start(&self) -> bool {
        matches!(self, Self::Available | Self::AvailableWithPermission)
    }
}
