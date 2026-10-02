//! What the layout inspection found, as the Flash activity and the card see
//! it — a summary, never the files (plan `lp2025/2026-10-01-1843-c6-repartition`).
//!
//! The effects layer reads the board's flash, classifies it, builds the
//! plan (and, for a migration, stores the backup) — and hands the model only
//! this. The bytes stay with the effects layer, the same rule as a push's
//! payload.

use serde::{Deserialize, Serialize};

/// The inspection's answer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayoutVerdict {
    /// Nothing about the board's files changes: write the firmware, as
    /// every update before the repartition did. No dialog.
    Plain,
    /// The board's files move to the new layout. Asks first.
    Migrate {
        files: u32,
        bytes: u64,
        /// Blocks left free on the new filesystem.
        free_blocks: u32,
        /// Fewer than a quarter free: allowed, but worth saying.
        tight: bool,
        /// The backup was stored in this browser and read back. `false`:
        /// storing failed, and Continue waits for the user to download it.
        backup_stored: bool,
        /// The uid the board must come back with (its `/.lp/device.json`).
        device_uid: Option<String>,
    },
    /// A stored backup's files go back onto the board. Asks first.
    Restore {
        /// When the backup was taken (whole epoch seconds).
        captured_at: u64,
        files: u32,
        bytes: u64,
        device_uid: Option<String>,
    },
    /// The files do not fit the new layout. Nothing is written; the board
    /// goes back to its old firmware with every file.
    Refused {
        files: u32,
        bytes: u64,
        /// Room the new filesystem has, in bytes.
        room_bytes: u64,
    },
}

impl LayoutVerdict {
    /// Does running this verdict need the user's yes first?
    pub fn needs_consent(&self) -> bool {
        matches!(self, Self::Migrate { .. } | Self::Restore { .. })
    }

    /// Does the write carry the board's files (a plan, not a plain flash)?
    pub fn carries_files(&self) -> bool {
        self.needs_consent()
    }

    /// The uid a carried write must bring back.
    pub fn expected_uid(&self) -> Option<&str> {
        match self {
            Self::Migrate { device_uid, .. } | Self::Restore { device_uid, .. } => {
                device_uid.as_deref()
            }
            Self::Plain | Self::Refused { .. } => None,
        }
    }
}

/// The activity's layout step, as the card shows it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FlashLayoutView {
    pub verdict: LayoutVerdict,
    /// The activity is waiting for Continue or Cancel.
    pub awaiting_consent: bool,
}
