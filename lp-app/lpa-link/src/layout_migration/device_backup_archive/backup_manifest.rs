//! The manifest written into every device backup archive.
//!
//! Support-facing, but shaped as if it were public — see the module README.
//! The fields answer the questions a restore asks before it writes anything:
//! *which board is this, which partition, whose device was it, and why was
//! it taken?*
//!
//! `device_uid` is the sharp one. `/.lp/device.json` lives INSIDE `lpfs`, so
//! it rides along in the archive; restoring a backup onto a different board
//! would clone an identity. `base_mac` (v2) is the board's permanent
//! identity, which a restore checks before it writes.

use serde::{Deserialize, Serialize};

/// The only archive format this build reads and writes.
///
/// Alpha posture (`docs/adr/2026-07-28-share-envelopes.md`'s house rule):
/// version and refuse, never migrate. v1 (July 2026) is refused.
pub const BACKUP_FORMAT_VERSION: u32 = 2;

/// Why a backup was taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupPurpose {
    /// A plain copy of the board's files.
    Backup,
    /// Taken before a partition-layout migration rewrote the board.
    LayoutMigration,
}

/// `manifest.json` at the root of a device backup archive.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    /// Epoch seconds at capture, from the app's injected clock.
    pub captured_at_epoch_seconds: f64,
    /// The device uid found at `/.lp/device.json` in the captured files, if
    /// the board had ever been stamped. Absent is honest.
    pub device_uid: Option<String>,
    /// The chip the bootloader named itself as during the read.
    pub chip: Option<String>,
    /// The board's factory base MAC (lowercase colon hex), read in the same
    /// bootloader session — the identity a restore checks.
    pub base_mac: Option<String>,
    /// Where the captured filesystem lived on the board.
    pub partition_offset: u32,
    pub partition_length: u32,
    /// Where a migration is putting it (absent for a plain backup).
    pub target_partition_offset: Option<u32>,
    pub target_partition_length: Option<u32>,
    /// littlefs block size the image was read at.
    pub block_size: u32,
    pub file_count: u32,
    /// Sum of the captured files' sizes — not the partition size.
    pub total_bytes: u64,
    pub purpose: BackupPurpose,
}

impl BackupManifest {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
