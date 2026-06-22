use serde::{Deserialize, Serialize};

use crate::LinkOperation;

/// Provider-neutral request for a low-level link management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkManagementRequest {
    /// Reset or reboot the endpoint/runtime without erasing user data.
    ResetRuntime,
    /// Flash the provider's configured firmware image.
    FlashFirmware,
    /// Erase device flash so the endpoint returns to a blank state.
    EraseDeviceFlash,
    /// Erase the raw device filesystem partition below the running server.
    EraseRawFilesystem,
}

impl LinkManagementRequest {
    pub fn operation(&self) -> LinkOperation {
        match self {
            Self::ResetRuntime => LinkOperation::Reset,
            Self::FlashFirmware => LinkOperation::FlashFirmware,
            Self::EraseDeviceFlash => LinkOperation::EraseDeviceFlash,
            Self::EraseRawFilesystem => LinkOperation::WriteRawFilesystem,
        }
    }
}
