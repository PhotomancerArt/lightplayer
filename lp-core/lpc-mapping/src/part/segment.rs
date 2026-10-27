//! A segment: one run of lamps on one port, a leaf of the rendered tree
//! (names.html "Segment"). Rendered from shapes, never stored; the only
//! thing that holds lamps.

use core::ops::Range;

use crate::part::lamp_ref::LampRef;
use crate::part::tree_id::TreeId;

/// One run of lamps.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub id: TreeId,
    /// Its lamps' positions: this range of [`crate::part::Tree::positions`],
    /// lamp 0 first (where the data enters, as drawn).
    pub lamps: Range<u32>,
    /// Its lamp facts, resolved down the tree (the mapping's, then each part
    /// above it that says).
    pub lamp: LampRef,
}

impl Segment {
    /// How many lamps it has.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.lamps.end - self.lamps.start
    }
}
