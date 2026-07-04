//! Plain-data snapshots of recovery state, safe to carry out of the
//! critical section and hand to reporting code (heartbeat, logs).
//!
//! Everything here is `Copy` and serde-free by design: wire conversion is
//! the caller's business.

use crate::crash_record::{CompactFrameName, CrashCause, CrashMsg, CrashRecord, OomStats};
use crate::frame_path::{FramePath, MAX_FRAME_DEPTH};
use crate::frame_record::FrameRecord;
use crate::path_entry::PathEntry;
use crate::recovery_level::RecoveryLevel;
use crate::recovery_region::RecoveryRegion;
use crate::recovery_stack::current_path;
use crate::reset_cause::ResetCause;
use crate::tuning::PATH_SLOTS;

/// Point-in-time copy of the recovery state.
#[derive(Copy, Clone, Debug)]
pub struct RecoverySnapshot {
    /// Why the current boot happened.
    pub reset_cause: ResetCause,
    /// Current boot generation (increments every boot).
    pub generation: u32,
    /// Boots since the region was (re)initialized.
    pub boot_count: u32,
    /// Whether this run has passed the boot-complete milestone.
    pub boot_complete: bool,
    /// Live frame stack (display copies; identity via `stack_path`).
    pub stack: [FrameRecord; MAX_FRAME_DEPTH],
    pub stack_depth: u8,
    /// Identity path of the live stack.
    pub stack_path: FramePath,
    /// Most recent committed crash, if any (may be from an older boot).
    pub last_crash: Option<CrashSnapshot>,
    /// Device recovery level (worst active ledger entry).
    pub level: RecoveryLevel,
    /// Whether this boot is running in safe mode territory (boot-loop
    /// counter at/over threshold).
    pub safe_mode: bool,
    pub consecutive_incomplete_boots: u32,
    /// Blame-ledger entries (empty slots included; filter on `is_empty`).
    pub path_entries: [PathEntry; PATH_SLOTS],
}

impl RecoverySnapshot {
    pub(crate) fn capture(region: &RecoveryRegion, reset_cause: ResetCause) -> Self {
        let depth = (region.depth() as usize).min(MAX_FRAME_DEPTH);
        let crash = region.crash();
        let ledger = region.ledger();
        Self {
            reset_cause,
            generation: region.generation(),
            boot_count: region.boot_count(),
            boot_complete: region.boot_complete(),
            stack: *region.frames(),
            stack_depth: depth as u8,
            stack_path: current_path(region),
            last_crash: crash
                .is_final()
                .then(|| CrashSnapshot::from_record(crash, region.generation())),
            level: ledger.device_level(),
            safe_mode: ledger.safe_mode(),
            consecutive_incomplete_boots: ledger.consecutive_incomplete_boots(),
            path_entries: *ledger.entries(),
        }
    }
}

/// Plain-data copy of a committed crash record.
#[derive(Copy, Clone, Debug)]
pub struct CrashSnapshot {
    pub cause: CrashCause,
    pub msg: CrashMsg,
    pub heap: OomStats,
    pub path: FramePath,
    pub path_names: [CompactFrameName; MAX_FRAME_DEPTH],
    pub pc_frames: [u32; 8],
    pub pc_count: u8,
    /// Boot generation the crash happened in.
    pub generation: u32,
    /// How many boots ago that was (0 = crashed during the current boot).
    pub boots_ago: u32,
}

impl CrashSnapshot {
    pub(crate) fn from_record(record: &CrashRecord, current_generation: u32) -> Self {
        let pcs = record.pc_frames();
        let mut pc_frames = [0u32; 8];
        pc_frames[..pcs.len()].copy_from_slice(pcs);
        let mut path_names = [CompactFrameName::EMPTY; MAX_FRAME_DEPTH];
        path_names[..record.path_names().len()].copy_from_slice(record.path_names());
        Self {
            cause: record.cause(),
            msg: *record.msg(),
            heap: record.heap(),
            path: record.path(),
            path_names,
            pc_frames,
            pc_count: pcs.len() as u8,
            generation: record.generation(),
            boots_ago: current_generation.wrapping_sub(record.generation()),
        }
    }

    /// Display wrapper for the crashed path, e.g.
    /// `boot / project:demo / node:fire`.
    pub fn path_display(&self) -> PathNames<'_> {
        PathNames(&self.path_names[..self.path.len()])
    }
}

/// One frame's display info, borrowed from a snapshot.
#[derive(Copy, Clone, Debug)]
pub struct FrameNameRef<'a> {
    pub kind_label: &'static str,
    pub name: &'a str,
}

/// `Display` over a crashed path's compact names.
pub struct PathNames<'a>(pub(crate) &'a [CompactFrameName]);

impl<'a> PathNames<'a> {
    pub fn iter(&self) -> impl Iterator<Item = FrameNameRef<'a>> + '_ {
        self.0.iter().map(|n| FrameNameRef {
            kind_label: n.kind().map_or("?", |k| k.as_str()),
            name: n.name(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl core::fmt::Display for PathNames<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_empty() {
            return f.write_str("<no frame>");
        }
        for (i, frame) in self.iter().enumerate() {
            if i > 0 {
                f.write_str("/")?;
            }
            if frame.name.is_empty() || frame.name == frame.kind_label {
                f.write_str(frame.kind_label)?;
            } else {
                write!(f, "{}:{}", frame.kind_label, frame.name)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;
    use crate::frame_kind::FrameKind;

    #[test]
    fn path_display_formats_kinds_and_names() {
        let mut names = [CompactFrameName::EMPTY; 3];
        names[0].set(FrameKind::Boot as u8, "boot");
        names[1].set(FrameKind::ProjectLoad as u8, "demo");
        names[2].set(FrameKind::NodeRender as u8, "fire");
        let display = PathNames(&names).to_string();
        assert_eq!(display, "boot/project:demo/node:fire");
    }

    #[test]
    fn empty_path_displays_placeholder() {
        assert_eq!(PathNames(&[]).to_string(), "<no frame>");
    }
}
