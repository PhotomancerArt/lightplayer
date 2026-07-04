//! One blame-ledger slot: crash accounting for a single path prefix.

use crate::frame_kind::FrameKind;
use crate::frame_record::truncation_boundary;

/// Bytes of leaf display name kept per ledger entry.
pub const ENTRY_NAME_CAP: usize = 16;

pub(crate) const ENTRY_STATE_EMPTY: u8 = 0;
pub(crate) const ENTRY_STATE_YELLOW: u8 = 1;
pub(crate) const ENTRY_STATE_RED: u8 = 2;

/// Blame state of one path prefix. Fixed 40 bytes, region-resident.
///
/// The entry is keyed by `path_hash` (order-sensitive hash of the whole
/// prefix); `kind`/`name` describe the prefix's leaf frame for reports.
/// `last_child_hash`/`distinct_children` implement the escalation
/// heuristic: a parent that has seen crashes under two *different*
/// children is gated itself. Tracking only the last child hash undercounts
/// alternating patterns (a,b,a,b counts 2 — enough) but costs 9 bytes
/// instead of a child list.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct PathEntry {
    path_hash: u64,
    last_child_hash: u64,
    state: u8,
    depth: u8,
    crash_count: u8,
    distinct_children: u8,
    clean_completions: u8,
    kind: u8,
    name_len: u8,
    _pad: u8,
    name: [u8; ENTRY_NAME_CAP],
}

impl PathEntry {
    pub const EMPTY: Self = Self {
        path_hash: 0,
        last_child_hash: 0,
        state: ENTRY_STATE_EMPTY,
        depth: 0,
        crash_count: 0,
        distinct_children: 0,
        clean_completions: 0,
        kind: 0,
        name_len: 0,
        _pad: 0,
        name: [0; ENTRY_NAME_CAP],
    };

    pub fn is_empty(&self) -> bool {
        self.state == ENTRY_STATE_EMPTY
    }

    pub fn is_yellow(&self) -> bool {
        self.state == ENTRY_STATE_YELLOW
    }

    pub fn is_red(&self) -> bool {
        self.state == ENTRY_STATE_RED
    }

    pub fn path_hash(&self) -> u64 {
        self.path_hash
    }

    pub fn depth(&self) -> u8 {
        self.depth
    }

    pub fn crash_count(&self) -> u8 {
        self.crash_count
    }

    pub fn distinct_children(&self) -> u8 {
        self.distinct_children
    }

    pub fn clean_completions(&self) -> u8 {
        self.clean_completions
    }

    pub fn kind(&self) -> Option<FrameKind> {
        FrameKind::from_u8(self.kind)
    }

    /// Leaf display name (truncated); `""` for the boot/root-level frame of
    /// an entry whose name did not survive corruption.
    pub fn name(&self) -> &str {
        let len = (self.name_len as usize).min(ENTRY_NAME_CAP);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    pub(crate) fn init(&mut self, path_hash: u64, depth: u8, kind_raw: u8, name: &str) {
        *self = Self::EMPTY;
        self.path_hash = path_hash;
        self.depth = depth;
        self.kind = kind_raw;
        let end = truncation_boundary(name, ENTRY_NAME_CAP);
        self.name_len = end as u8;
        self.name[..end].copy_from_slice(&name.as_bytes()[..end]);
        self.state = ENTRY_STATE_YELLOW;
    }

    pub(crate) fn set_state(&mut self, state: u8) {
        self.state = state;
    }

    pub(crate) fn record_crash_hit(&mut self) {
        self.crash_count = self.crash_count.saturating_add(1);
        self.clean_completions = 0;
    }

    pub(crate) fn note_child(&mut self, child_hash: u64) {
        if self.last_child_hash != child_hash {
            self.distinct_children = self.distinct_children.saturating_add(1);
            self.last_child_hash = child_hash;
        }
    }

    pub(crate) fn note_clean_completion(&mut self) -> u8 {
        self.clean_completions = self.clean_completions.saturating_add(1);
        self.clean_completions
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::EMPTY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_is_40_bytes() {
        assert_eq!(core::mem::size_of::<PathEntry>(), 40);
    }

    #[test]
    fn init_starts_yellow_with_identity_and_name() {
        let mut e = PathEntry::EMPTY;
        e.init(
            0xdead_beef,
            3,
            FrameKind::NodeRender as u8,
            "nodes/fire-storm-xl",
        );
        assert!(e.is_yellow());
        assert_eq!(e.path_hash(), 0xdead_beef);
        assert_eq!(e.depth(), 3);
        assert_eq!(e.kind(), Some(FrameKind::NodeRender));
        assert_eq!(e.name(), "nodes/fire-storm"); // truncated to 16
        assert_eq!(e.crash_count(), 0);
    }

    #[test]
    fn distinct_children_counts_changes_only() {
        let mut e = PathEntry::EMPTY;
        e.init(1, 1, FrameKind::ProjectLoad as u8, "p");
        e.note_child(100);
        e.note_child(100);
        assert_eq!(e.distinct_children(), 1);
        e.note_child(200);
        assert_eq!(e.distinct_children(), 2);
    }

    #[test]
    fn crash_hit_resets_clean_completions() {
        let mut e = PathEntry::EMPTY;
        e.init(1, 1, FrameKind::NodeRender as u8, "n");
        e.note_clean_completion();
        e.note_clean_completion();
        e.record_crash_hit();
        assert_eq!(e.clean_completions(), 0);
        assert_eq!(e.crash_count(), 1);
    }
}
