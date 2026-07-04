//! Convert `lp_recovery` snapshots into wire `RecoveryStatus`.
//!
//! Lives here (not in lp-recovery) so the recovery crate stays serde-free
//! and zero-alloc; formatting/allocation is a reporting concern.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lp_recovery::{RecoveryLevel, RecoverySnapshot};
use lpc_wire::server::{CrashSummaryWire, RecoveryLevelWire, RecoveryPathWire, RecoveryStatus};

/// Current recovery status for heartbeat reporting, if a recovery global
/// is installed on this target.
pub fn current_recovery_status() -> Option<RecoveryStatus> {
    lp_recovery::snapshot().map(|snapshot| recovery_status_from_snapshot(&snapshot))
}

pub fn recovery_status_from_snapshot(snapshot: &RecoverySnapshot) -> RecoveryStatus {
    let level = match snapshot.level {
        RecoveryLevel::Green => RecoveryLevelWire::Green,
        RecoveryLevel::Yellow => RecoveryLevelWire::Yellow,
        RecoveryLevel::Red => RecoveryLevelWire::Red,
    };
    let last_crash = snapshot.last_crash.as_ref().map(|crash| CrashSummaryWire {
        cause: crash.cause.as_str().to_string(),
        path: crash.path_display().to_string(),
        message: crash.msg.as_str().to_string(),
        boots_ago: crash.boots_ago,
    });
    let paths: Vec<RecoveryPathWire> = snapshot
        .path_entries
        .iter()
        .filter(|entry| !entry.is_empty())
        .map(|entry| RecoveryPathWire {
            path: entry_path_label(entry),
            state: if entry.is_red() { "red" } else { "yellow" }.to_string(),
            crash_count: entry.crash_count(),
        })
        .collect();
    RecoveryStatus {
        level,
        reset_reason: snapshot.reset_cause.as_str().to_string(),
        boot_count: snapshot.boot_count,
        safe_mode: snapshot.safe_mode,
        last_crash,
        paths,
    }
}

/// Leaf label for a ledger entry, e.g. `node:nodes/fire`. Ledger entries
/// keep only the leaf frame's kind + truncated name (identity is hashed);
/// the crash summary carries the full path when one is available.
fn entry_path_label(entry: &lp_recovery::PathEntry) -> String {
    let kind = entry.kind().map_or("?", |kind| kind.as_str());
    if entry.name().is_empty() {
        kind.to_string()
    } else {
        format!("{kind}:{}", entry.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_recovery::{
        CrashCause, FrameKind, InMemoryBackend, Recovery, RecoveryHandle, ResetCause,
    };

    #[test]
    fn snapshot_converts_to_wire_status() {
        let (mut recovery, _) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
        recovery.mark_boot_complete();
        let frame = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/fire")
            .unwrap();
        recovery.stage_crash(CrashCause::Panic, &"boom", Some(("n.rs", 3)), &[], None);
        recovery.finalize_crash_and_reset();
        let (mut recovery, _) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        let _ = frame;

        let status = recovery_status_from_snapshot(&recovery.snapshot());
        assert_eq!(status.level, RecoveryLevelWire::Yellow);
        assert_eq!(status.reset_reason, "software-reset");
        assert!(!status.safe_mode);
        let crash = status.last_crash.expect("last crash");
        assert_eq!(crash.cause, "panic");
        assert_eq!(crash.path, "node:nodes/fire");
        assert_eq!(crash.message, "boom (at n.rs:3)");
        assert_eq!(crash.boots_ago, 1);
        assert_eq!(status.paths.len(), 1);
        assert_eq!(status.paths[0].path, "node:nodes/fire");
        assert_eq!(status.paths[0].state, "yellow");
    }
}
