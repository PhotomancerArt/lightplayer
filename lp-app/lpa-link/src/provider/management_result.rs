use serde::{Deserialize, Serialize};

use crate::LinkManagementProgress;

/// Firmware image summary reported by a provider management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkFirmwareManifest {
    pub firmware_id: String,
    pub display_name: String,
    pub target_chip: String,
    pub image_count: u32,
    pub total_bytes: u32,
    pub manifest_path: Option<String>,
}

/// Result of flashing firmware through a link provider.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkFirmwareFlashResult {
    pub manifest: LinkFirmwareManifest,
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<LinkManagementProgress>,
}

/// Result of erasing an endpoint back to a blank state.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkEraseDeviceResult {
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<LinkManagementProgress>,
}

/// Result of erasing a raw device filesystem partition.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkRawFilesystemEraseResult {
    pub logs: Vec<String>,
    pub progress: Vec<LinkManagementProgress>,
}

/// Provider-neutral result from a link management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkManagementResult {
    ResetRuntime,
    FlashFirmware(LinkFirmwareFlashResult),
    EraseDeviceFlash(LinkEraseDeviceResult),
    EraseRawFilesystem(LinkRawFilesystemEraseResult),
}
