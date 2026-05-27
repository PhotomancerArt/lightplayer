//! Snapshot diff expressed as overlay pending state per artifact path.

use alloc::vec::Vec;

use lpfs::LpPathBuf;

use super::ArtifactEdits;

/// Pending overlay edits keyed by absolute artifact path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverlayDelta {
    edits: Vec<(LpPathBuf, ArtifactEdits)>,
}

impl OverlayDelta {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn insert(&mut self, path: LpPathBuf, edits: ArtifactEdits) {
        if edits.is_empty() {
            self.edits.retain(|(existing, _)| existing != &path);
            return;
        }
        if let Some((_, existing)) = self
            .edits
            .iter_mut()
            .find(|(existing, _)| existing == &path)
        {
            *existing = edits;
        } else {
            self.edits.push((path, edits));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&LpPathBuf, &ArtifactEdits)> {
        self.edits.iter().map(|(path, edits)| (path, edits))
    }
}
