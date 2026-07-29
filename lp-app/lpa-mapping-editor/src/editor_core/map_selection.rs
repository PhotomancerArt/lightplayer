//! Editor selection: object indices plus an optional vertex sub-selection.
//!
//! Documents deliberately carry no object ids (the schema stays clean), so
//! selection is by wiring-order index and the session remaps it on every
//! structural edit (delete/reorder) — those fix-ups are host-tested.

use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapSelection {
    /// Selected object indices (wiring order).
    pub objects: BTreeSet<usize>,
    /// Selected vertex of a single selected path object.
    pub vertex: Option<usize>,
}

impl MapSelection {
    pub fn clear(&mut self) {
        self.objects.clear();
        self.vertex = None;
    }

    pub fn select_only(&mut self, index: usize) {
        self.objects.clear();
        self.objects.insert(index);
        self.vertex = None;
    }

    pub fn toggle(&mut self, index: usize) {
        if !self.objects.remove(&index) {
            self.objects.insert(index);
        }
        self.vertex = None;
    }

    #[must_use]
    pub fn single(&self) -> Option<usize> {
        (self.objects.len() == 1)
            .then(|| self.objects.iter().next().copied())
            .flatten()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}
