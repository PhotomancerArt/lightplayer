//! The manifest written into every filesystem backup.
//!
//! Support-facing, but shaped as if it were public — see the module README.
//! The fields exist to answer the questions a restore has to ask before it
//! writes anything: *which board is this, which partition, and whose device
//! was it?*
//!
//! `device_uid` is the sharp one. `/.lp/device.json` lives INSIDE `lpfs`, so
//! it rides along in the archive; restoring a backup onto a different board
//! would clone an identity. Recording the captured uid here is what lets M7
//! detect a cross-device restore instead of silently performing one.

use serde::{Deserialize, Serialize};

/// Bumped when the archive layout changes in a way a reader must notice.
///
/// Alpha posture (`docs/adr/2026-07-28-share-envelopes.md`'s house rule):
/// version and refuse, never migrate.
pub const BACKUP_FORMAT_VERSION: u32 = 1;

/// `manifest.json` at the root of a filesystem backup archive.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    /// Epoch seconds at capture, from the app's injected clock.
    pub captured_at_epoch_seconds: f64,
    /// The device uid found at `/.lp/device.json` in the captured image, if
    /// the board had ever been stamped. Absent is honest: a board that was
    /// never named has none.
    pub device_uid: Option<String>,
    /// The chip the bootloader named itself as during the read.
    pub chip: Option<String>,
    /// Where the captured partition lives on that chip.
    pub partition_offset: u32,
    pub partition_length: u32,
    /// littlefs block size the image was read at.
    pub block_size: u32,
    pub file_count: u32,
    /// Sum of the captured files' sizes — not the partition size.
    pub total_bytes: u64,
}

impl BackupManifest {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
