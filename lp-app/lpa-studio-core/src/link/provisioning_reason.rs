use serde::{Deserialize, Serialize};

/// Why Studio cannot yet open a normal LightPlayer server session.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProvisioningReason {
    DeviceBlank,
    BootloaderMode,
    FirmwareMissing,
    FirmwareIncompatible { version: Option<String> },
    ServerUnavailable,
    UserRequested,
    Other { message: String },
}
