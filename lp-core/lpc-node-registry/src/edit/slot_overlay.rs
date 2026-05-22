//! Path-keyed pending artifact state.
//!
//! [`SlotOverlay`] holds uncommitted edits keyed by absolute project path.
//! Slot edits are stored as parsed drafts; assets as raw bytes or deletion
//! markers. Cleared after a successful [`crate::NodeDefRegistry::commit`].

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpfs::{LpPath, LpPathBuf};

use super::def_draft::DefDraft;

/// Pending state for one absolute project path.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotOverlayEntry {
    Deleted,
    Bytes(Vec<u8>),
    DefDraft(DefDraft),
}

/// In-memory scratch for uncommitted client edits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotOverlay {
    by_path: BTreeMap<String, SlotOverlayEntry>,
}

impl SlotOverlay {
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
            SlotOverlayEntry::Bytes(bytes) => Some(bytes.as_slice()),
            SlotOverlayEntry::Deleted | SlotOverlayEntry::DefDraft(_) => None,
        }
    }

    pub fn entry(&self, path: &LpPath) -> Option<&SlotOverlayEntry> {
        self.by_path.get(path.as_str())
    }

    pub fn clear(&mut self) {
        self.by_path.clear();
    }

    /// Remove pending state for `path`. Returns whether an entry existed.
    pub fn remove_path(&mut self, path: &LpPath) -> bool {
        self.by_path.remove(path.as_str()).is_some()
    }

    /// Iterate pending paths and entries in stable order.
    pub(crate) fn iter_entries(&self) -> impl Iterator<Item = (LpPathBuf, &SlotOverlayEntry)> {
        self.by_path
            .iter()
            .map(|(path, entry)| (LpPathBuf::from(path.as_str()), entry))
    }

    pub(crate) fn apply_bytes(&mut self, path: LpPathBuf, bytes: Vec<u8>) {
        self.by_path
            .insert(path.as_str().to_string(), SlotOverlayEntry::Bytes(bytes));
    }

    pub(crate) fn apply_delete(&mut self, path: LpPathBuf) {
        self.by_path
            .insert(path.as_str().to_string(), SlotOverlayEntry::Deleted);
    }

    pub(crate) fn apply_def_draft(&mut self, path: LpPathBuf, draft: DefDraft) {
        self.by_path
            .insert(path.as_str().to_string(), SlotOverlayEntry::DefDraft(draft));
    }
}
