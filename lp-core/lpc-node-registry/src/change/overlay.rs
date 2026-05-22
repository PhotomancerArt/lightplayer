//! Path-keyed pending artifact state.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpfs::{LpPath, LpPathBuf};

use super::slot_draft::SlotDraft;

/// Pending state for one absolute project path.
#[derive(Clone, Debug, PartialEq)]
pub enum OverlayEntry {
    Deleted,
    Bytes(Vec<u8>),
    SlotDraft(SlotDraft),
}

/// In-memory scratch for uncommitted client edits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChangeOverlay {
    by_path: BTreeMap<String, OverlayEntry>,
}

impl ChangeOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    pub fn contains_path(&self, path: &LpPath) -> bool {
        self.by_path.contains_key(path.as_str())
    }

    pub fn get_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        match self.by_path.get(path.as_str())? {
            OverlayEntry::Bytes(bytes) => Some(bytes.as_slice()),
            OverlayEntry::Deleted | OverlayEntry::SlotDraft(_) => None,
        }
    }

    pub fn entry(&self, path: &LpPath) -> Option<&OverlayEntry> {
        self.by_path.get(path.as_str())
    }

    pub fn clear(&mut self) {
        self.by_path.clear();
    }

    /// Iterate pending paths and entries in stable order.
    pub(crate) fn iter_entries(&self) -> impl Iterator<Item = (LpPathBuf, &OverlayEntry)> {
        self.by_path
            .iter()
            .map(|(path, entry)| (LpPathBuf::from(path.as_str()), entry))
    }

    pub(crate) fn apply_bytes(&mut self, path: LpPathBuf, bytes: Vec<u8>) {
        self.by_path
            .insert(path.as_str().to_string(), OverlayEntry::Bytes(bytes));
    }

    pub(crate) fn apply_delete(&mut self, path: LpPathBuf) {
        self.by_path
            .insert(path.as_str().to_string(), OverlayEntry::Deleted);
    }

    pub(crate) fn apply_slot_draft(&mut self, path: LpPathBuf, draft: SlotDraft) {
        self.by_path
            .insert(path.as_str().to_string(), OverlayEntry::SlotDraft(draft));
    }
}
