//! Wire representation of the crash-recovery state (see `lp-recovery`).
//!
//! Carried in the periodic `Heartbeat` so clients can show the device's
//! recovery level and the last crash without an extra request. Conversion
//! from `lp_recovery::RecoverySnapshot` lives server-side (lpa-server);
//! this crate stays a plain data schema.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Device-wide recovery level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryLevelWire {
    /// No failures under watch.
    Green,
    /// At least one path crashed recently and is under watch.
    Yellow,
    /// At least one path is disabled (gated) after repeated crashes.
    Red,
}

/// Summary of the most recent committed crash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashSummaryWire {
    /// "panic" | "oom" | "watchdog" | "unknown".
    pub cause: String,
    /// Human-legible frame path, e.g. `boot/project:demo/node:nodes/fire`.
    pub path: String,
    /// Truncated crash message (empty for watchdog-attributed crashes).
    pub message: String,
    /// How many boots ago the crash happened (0 = this boot).
    pub boots_ago: u32,
}

/// One blame-ledger entry: a path under watch (yellow) or gated (red).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryPathWire {
    /// Leaf frame of the watched path, e.g. `node:nodes/fire`.
    pub path: String,
    /// "yellow" | "red".
    pub state: String,
    pub crash_count: u8,
}

/// Recovery state as reported in the heartbeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub level: RecoveryLevelWire,
    /// Why the current boot happened ("power-on", "watchdog-reset", ...).
    pub reset_reason: String,
    /// Boots since the recovery region was (re)initialized.
    pub boot_count: u32,
    /// Whether this boot skipped project auto-load after repeated
    /// incomplete boots.
    pub safe_mode: bool,
    #[serde(default)]
    pub last_crash: Option<CrashSummaryWire>,
    /// Active blame-ledger entries (yellow and red).
    #[serde(default)]
    pub paths: Vec<RecoveryPathWire>,
}
