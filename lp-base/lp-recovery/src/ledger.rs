//! The blame ledger: crash accounting, escalation, gating, safe mode.
//!
//! Pure bookkeeping + queries over region-resident state. Enforcement
//! (skipping loads, surfacing errors) is the caller's job.

use crate::crash_record::CompactFrameName;
use crate::frame_kind::FrameKind;
use crate::frame_path::FramePath;
use crate::path_entry::{ENTRY_NAME_CAP, ENTRY_STATE_RED, ENTRY_STATE_YELLOW, PathEntry};
use crate::recovery_level::RecoveryLevel;
use crate::reset_cause::ResetCause;
use crate::tuning::{
    CLEAN_COMPLETIONS_TO_GREEN, DISTINCT_CHILDREN_TO_ESCALATE, INCOMPLETE_BOOTS_TO_SAFE_MODE,
    PATH_SLOTS,
};

/// Why an `enter` was denied: the blocking entry's identity, for messages
/// like `recovery: disabled after 2 crashes in shader-compile 'fire.glsl'`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct GatedInfo {
    pub kind: Option<FrameKind>,
    pub crash_count: u8,
    pub depth: u8,
    name_len: u8,
    name: [u8; ENTRY_NAME_CAP],
}

impl GatedInfo {
    fn from_entry(entry: &PathEntry) -> Self {
        let name = entry.name();
        let mut buf = [0u8; ENTRY_NAME_CAP];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        Self {
            kind: entry.kind(),
            crash_count: entry.crash_count(),
            depth: entry.depth(),
            name_len: name.len() as u8,
            name: buf,
        }
    }

    pub fn name(&self) -> &str {
        let len = (self.name_len as usize).min(ENTRY_NAME_CAP);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }
}

impl core::fmt::Display for GatedInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let kind = self.kind.map_or("?", |k| k.as_str());
        let crashes = self.crash_count;
        if self.name().is_empty() {
            write!(f, "{kind} (disabled after {crashes} crashes)")
        } else {
            write!(
                f,
                "{kind} '{}' (disabled after {crashes} crashes)",
                self.name()
            )
        }
    }
}

/// Region-resident ledger state. Not CRC-covered: crash-time updates may be
/// interrupted by a watchdog reset by design; fields are small integers and
/// a torn entry degrades to slightly-wrong counts, never to unsafety.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Ledger {
    consecutive_incomplete_boots: u32,
    _pad: u32,
    entries: [PathEntry; PATH_SLOTS],
}

impl Ledger {
    pub const EMPTY: Self = Self {
        consecutive_incomplete_boots: 0,
        _pad: 0,
        entries: [PathEntry::EMPTY; PATH_SLOTS],
    };

    // --- boot-time transitions ---------------------------------------------

    /// Start-of-boot pass, run BEFORE the prior run's crash (if any) is
    /// recorded: demote red entries to yellow (one retry per boot) and
    /// account for boot-loop detection.
    pub(crate) fn on_boot(&mut self, prior_boot_complete: bool, cause: ResetCause) {
        for entry in &mut self.entries {
            if entry.is_red() {
                entry.set_state(ENTRY_STATE_YELLOW);
            }
        }
        if prior_boot_complete {
            self.consecutive_incomplete_boots = 0;
        } else if cause.blames_code() {
            self.consecutive_incomplete_boots = self.consecutive_incomplete_boots.saturating_add(1);
        }
        // Incomplete boot without code blame (user reset mid-boot, brownout):
        // neither count nor forgive.
    }

    /// Whether this boot should skip crash-prone work (project auto-load).
    pub fn safe_mode(&self) -> bool {
        self.consecutive_incomplete_boots >= INCOMPLETE_BOOTS_TO_SAFE_MODE
    }

    pub fn consecutive_incomplete_boots(&self) -> u32 {
        self.consecutive_incomplete_boots
    }

    // --- crash accounting ---------------------------------------------------

    /// Record a crash on `path` (and every prefix of it), applying yellow/
    /// red transitions and hierarchical escalation.
    ///
    /// `names` supplies display names per depth (index `d` names the frame
    /// at depth `d+1`, as in crash records); shorter-than-path or missing
    /// names are fine — identity comes from the path hashes.
    ///
    /// Crashes with an empty path (hang outside any frame) are NOT ledger
    /// material: an empty path is a prefix of everything, so gating it would
    /// gate the whole device. Boot-loop accounting covers that case instead.
    pub(crate) fn record_crash(&mut self, path: &FramePath, names: &[CompactFrameName]) {
        let total = path.len();
        for depth in 1..=total {
            let hash = path.prefix_hash(depth);
            let kind_raw = path.entry(depth - 1).map_or(0, |(kind, _)| kind);
            let name = names.get(depth - 1).map_or("", |n| n.name());
            let Some(idx) = self.find_or_create(hash, depth as u8, kind_raw, name) else {
                continue; // slot pressure: crash on this prefix goes unrecorded
            };
            let is_exact = depth == total;
            let child_hash = (!is_exact).then(|| path.prefix_hash(depth + 1));

            let entry = &mut self.entries[idx];
            let was_yellow = entry.is_yellow();
            entry.record_crash_hit();
            if let Some(child) = child_hash {
                entry.note_child(child);
            }
            // Exact path: second crash while under watch gates it.
            if is_exact && was_yellow && entry.crash_count() >= 2 {
                entry.set_state(ENTRY_STATE_RED);
            }
            // Parent: gated only when distinct children keep crashing.
            if entry.distinct_children() >= DISTINCT_CHILDREN_TO_ESCALATE {
                entry.set_state(ENTRY_STATE_RED);
            }
        }
    }

    /// A watched (yellow) path completed cleanly; enough of these clears it.
    pub(crate) fn record_clean_completion(&mut self, path: &FramePath) {
        let hash = path.full_hash();
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| !e.is_empty() && e.path_hash() == hash)
        else {
            return;
        };
        if entry.is_yellow() && entry.note_clean_completion() >= CLEAN_COMPLETIONS_TO_GREEN {
            entry.clear();
        }
        // Red entries ignore completions — they cannot have run.
    }

    // --- queries -------------------------------------------------------------

    /// Deny when `candidate` (a would-be stack path, leaf included) or any
    /// of its prefixes is gated red.
    pub(crate) fn check_enter(&self, candidate: &FramePath) -> Option<GatedInfo> {
        for depth in 1..=candidate.len() {
            let hash = candidate.prefix_hash(depth);
            if let Some(entry) = self
                .entries
                .iter()
                .find(|e| e.is_red() && e.path_hash() == hash)
            {
                return Some(GatedInfo::from_entry(entry));
            }
        }
        None
    }

    /// Worst active entry state.
    pub fn device_level(&self) -> RecoveryLevel {
        let mut level = RecoveryLevel::Green;
        for entry in &self.entries {
            if entry.is_red() {
                return RecoveryLevel::Red;
            }
            if entry.is_yellow() {
                level = RecoveryLevel::Yellow;
            }
        }
        level
    }

    pub fn entries(&self) -> &[PathEntry; PATH_SLOTS] {
        &self.entries
    }

    // --- internals -----------------------------------------------------------

    /// Find the entry keyed `hash`, or claim a slot for it. `None` when the
    /// ledger is full of red entries (which are never evicted).
    fn find_or_create(&mut self, hash: u64, depth: u8, kind_raw: u8, name: &str) -> Option<usize> {
        if let Some(idx) = self
            .entries
            .iter()
            .position(|e| !e.is_empty() && e.path_hash() == hash)
        {
            return Some(idx);
        }
        let idx = match self.entries.iter().position(|e| e.is_empty()) {
            Some(idx) => idx,
            // Evict the yellow entry closest to green (most clean
            // completions); red entries stay.
            None => self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.is_yellow())
                .max_by_key(|(_, e)| e.clean_completions())
                .map(|(idx, _)| idx)?,
        };
        self.entries[idx].init(hash, depth, kind_raw, name);
        Some(idx)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;
    use crate::tuning;

    /// Build a path from (kind, hash) pairs.
    fn path(entries: &[(FrameKind, u32)]) -> FramePath {
        let mut p = FramePath::EMPTY;
        for &(kind, hash) in entries {
            assert!(p.push(kind, hash));
        }
        p
    }

    /// Display names "f0".."f7" per depth (kind comes from the path itself).
    fn names() -> [CompactFrameName; 8] {
        let labels = ["f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7"];
        let mut out = [CompactFrameName::EMPTY; 8];
        for (i, label) in labels.iter().enumerate() {
            out[i].set(FrameKind::NodeRender as u8, label);
        }
        out
    }

    const A: (FrameKind, u32) = (FrameKind::Boot, 0xa);
    const B: (FrameKind, u32) = (FrameKind::ProjectLoad, 0xb);
    const C: (FrameKind, u32) = (FrameKind::NodeRender, 0xc);
    const F: (FrameKind, u32) = (FrameKind::NodeRender, 0xf);
    const G: (FrameKind, u32) = (FrameKind::NodeRender, 0x9);

    #[test]
    fn first_crash_yellow_second_red_and_gated() {
        let mut ledger = Ledger::EMPTY;
        let p = path(&[A, B, C]);

        ledger.record_crash(&p, &names());
        assert_eq!(ledger.device_level(), RecoveryLevel::Yellow);
        assert!(ledger.check_enter(&p).is_none(), "yellow does not gate");

        ledger.record_crash(&p, &names());
        assert_eq!(ledger.device_level(), RecoveryLevel::Red);
        let gated = ledger.check_enter(&p).expect("red gates the exact path");
        assert_eq!(gated.crash_count, 2);
        assert_eq!(gated.name(), "f2");
        assert!(gated.to_string().contains("'f2'"));

        // Parents saw the same single child twice: not gated.
        assert!(ledger.check_enter(&path(&[A, B])).is_none());
        // Deeper paths under the red one ARE gated (prefix rule).
        let mut deeper = p;
        deeper.push(FrameKind::ShaderCompile, 0x5);
        assert!(ledger.check_enter(&deeper).is_some());
        // Sibling is unaffected.
        assert!(ledger.check_enter(&path(&[A, B, F])).is_none());
    }

    #[test]
    fn three_clean_completions_return_to_green() {
        let mut ledger = Ledger::EMPTY;
        let p = path(&[A, B, C]);
        ledger.record_crash(&p, &names());
        assert_eq!(ledger.device_level(), RecoveryLevel::Yellow);

        for _ in 0..tuning::CLEAN_COMPLETIONS_TO_GREEN {
            ledger.record_clean_completion(&p);
        }
        // The exact entry cleared; parent prefixes remain yellow until they
        // complete cleanly themselves.
        assert!(ledger.check_enter(&p).is_none());
        for _ in 0..tuning::CLEAN_COMPLETIONS_TO_GREEN {
            ledger.record_clean_completion(&path(&[A, B]));
            ledger.record_clean_completion(&path(&[A]));
        }
        assert_eq!(ledger.device_level(), RecoveryLevel::Green);
    }

    #[test]
    fn crash_resets_clean_progress() {
        let mut ledger = Ledger::EMPTY;
        let p = path(&[A, C]);
        ledger.record_crash(&p, &names());
        ledger.record_clean_completion(&p);
        ledger.record_clean_completion(&p);
        ledger.record_crash(&p, &names()); // second crash while yellow → red
        assert!(ledger.check_enter(&p).is_some());
    }

    #[test]
    fn distinct_children_escalate_to_parent() {
        let mut ledger = Ledger::EMPTY;
        ledger.record_crash(&path(&[A, B, C]), &names());
        ledger.record_crash(&path(&[A, B, F]), &names());

        // b is gated: anything under a→b is denied...
        let gated = ledger
            .check_enter(&path(&[A, B, G]))
            .expect("parent escalation gates new children too");
        assert_eq!(gated.depth, 2);
        // ...including b itself:
        assert!(ledger.check_enter(&path(&[A, B])).is_some());
        // ...but siblings of b are fine:
        assert!(
            ledger
                .check_enter(&path(&[A, (FrameKind::ProjectLoad, 0xbb)]))
                .is_none()
        );
        // a saw only one distinct child (b): not gated.
        assert!(ledger.check_enter(&path(&[A])).is_none());
    }

    #[test]
    fn escalation_walks_all_the_way_up() {
        let mut ledger = Ledger::EMPTY;
        // Two crashes under different project subtrees gate the boot frame's
        // child paths... a sees children b and bb → a itself red.
        ledger.record_crash(&path(&[A, B, C]), &names());
        ledger.record_crash(&path(&[A, (FrameKind::ProjectLoad, 0xbb), F]), &names());
        assert!(ledger.check_enter(&path(&[A])).is_some());
        assert_eq!(ledger.device_level(), RecoveryLevel::Red);
    }

    #[test]
    fn red_demotes_to_yellow_on_boot_for_one_retry() {
        let mut ledger = Ledger::EMPTY;
        let p = path(&[A, C]);
        ledger.record_crash(&p, &names());
        ledger.record_crash(&p, &names());
        assert!(ledger.check_enter(&p).is_some());

        ledger.on_boot(true, ResetCause::SoftwareReset);
        assert!(ledger.check_enter(&p).is_none(), "one retry per boot");
        assert_eq!(ledger.device_level(), RecoveryLevel::Yellow);

        // Crashing again during the retry re-gates immediately.
        ledger.record_crash(&p, &names());
        assert!(ledger.check_enter(&p).is_some());
    }

    #[test]
    fn boot_loop_counting_and_safe_mode() {
        let mut ledger = Ledger::EMPTY;
        assert!(!ledger.safe_mode());
        ledger.on_boot(false, ResetCause::SoftwareReset);
        assert!(!ledger.safe_mode());
        ledger.on_boot(false, ResetCause::WatchdogReset);
        assert!(ledger.safe_mode(), "two incomplete boots => safe mode");
        assert_eq!(ledger.consecutive_incomplete_boots(), 2);

        // A completed boot forgives.
        ledger.on_boot(true, ResetCause::SoftwareReset);
        assert!(!ledger.safe_mode());

        // Incomplete boot without code blame neither counts nor forgives.
        ledger.on_boot(false, ResetCause::SoftwareReset);
        ledger.on_boot(false, ResetCause::UserReset);
        assert_eq!(ledger.consecutive_incomplete_boots(), 1);
    }

    #[test]
    fn empty_path_crashes_are_not_ledger_material() {
        let mut ledger = Ledger::EMPTY;
        ledger.record_crash(&FramePath::EMPTY, &names());
        assert_eq!(ledger.device_level(), RecoveryLevel::Green);
        assert!(ledger.check_enter(&path(&[A])).is_none());
    }

    #[test]
    fn eviction_prefers_almost_green_never_red() {
        let mut ledger = Ledger::EMPTY;
        // Fill all slots with distinct yellow single-frame paths.
        for i in 0..tuning::PATH_SLOTS as u32 {
            ledger.record_crash(&path(&[(FrameKind::NodeRender, 100 + i)]), &names());
        }
        // Give slot for hash 100 the most clean completions (closest to green).
        ledger.record_clean_completion(&path(&[(FrameKind::NodeRender, 100)]));
        ledger.record_clean_completion(&path(&[(FrameKind::NodeRender, 100)]));

        // A new crashing path must evict the almost-green one.
        ledger.record_crash(&path(&[(FrameKind::NodeRender, 999)]), &names());
        let hash_100 = path(&[(FrameKind::NodeRender, 100)]).full_hash();
        assert!(
            ledger
                .entries()
                .iter()
                .all(|e| e.is_empty() || e.path_hash() != hash_100),
            "almost-green entry evicted"
        );
        let hash_999 = path(&[(FrameKind::NodeRender, 999)]).full_hash();
        assert!(ledger.entries().iter().any(|e| e.path_hash() == hash_999));
    }

    #[test]
    fn all_red_ledger_drops_new_entries_but_keeps_gates() {
        let mut ledger = Ledger::EMPTY;
        for i in 0..tuning::PATH_SLOTS as u32 {
            let p = path(&[(FrameKind::NodeRender, 200 + i)]);
            ledger.record_crash(&p, &names());
            ledger.record_crash(&p, &names()); // red
        }
        // New crash cannot claim a slot...
        ledger.record_crash(&path(&[(FrameKind::NodeRender, 777)]), &names());
        let hash_777 = path(&[(FrameKind::NodeRender, 777)]).full_hash();
        assert!(ledger.entries().iter().all(|e| e.path_hash() != hash_777));
        // ...and every red gate still stands.
        for i in 0..tuning::PATH_SLOTS as u32 {
            assert!(
                ledger
                    .check_enter(&path(&[(FrameKind::NodeRender, 200 + i)]))
                    .is_some()
            );
        }
    }
}
