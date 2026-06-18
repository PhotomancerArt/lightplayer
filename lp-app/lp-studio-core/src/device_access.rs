use lpa_link::LinkProviderId;
use serde::{Deserialize, Serialize};

/// Browser or host access state for a low-level device provider.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceAccess {
    pub provider_id: LinkProviderId,
    pub status: DeviceAccessStatus,
}

impl DeviceAccess {
    pub fn new(provider_id: impl Into<LinkProviderId>, status: DeviceAccessStatus) -> Self {
        Self {
            provider_id: provider_id.into(),
            status,
        }
    }
}

/// User/device permission state before a link endpoint can be connected.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum DeviceAccessStatus {
    Unknown,
    Unsupported { reason: String },
    PermissionRequired,
    PermissionDenied { reason: String },
    Granted,
}
