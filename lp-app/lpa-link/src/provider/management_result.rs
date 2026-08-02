use serde::{Deserialize, Serialize};

use crate::{LinkFlashRegion, LinkManagementProgress};

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

/// Result of reading the raw device filesystem partition back to the host.
///
/// `image` is the partition's bytes verbatim — a littlefs image, not files.
/// Parsing it is deliberately somebody else's job: `lpa-link` moves bytes off
/// a board that may not boot, and the filesystem format is the concern of the
/// layer that turns the image into an archive.
///
/// `region` and `chip_name` ride along because the read RESOLVED them (the
/// SYNC handshake names the chip; the chip picks the partition), and a
/// backup's manifest has to record which partition of which board it is or a
/// later restore cannot tell.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkRawFilesystemReadResult {
    pub image: Vec<u8>,
    pub region: LinkFlashRegion,
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<LinkManagementProgress>,
}

/// Result of writing the boot-control sector.
///
/// `flags` echoes what was written so callers can report the instruction
/// that actually landed rather than the one they asked for. The device does
/// not act on it until it next restarts.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkBootControlResult {
    pub flags: u32,
    pub chip_name: Option<String>,
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
    ReadRawFilesystem(LinkRawFilesystemReadResult),
    SetBootControl(LinkBootControlResult),
}
