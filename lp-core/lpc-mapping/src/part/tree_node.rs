//! One node of the rendered tree, and what made it.

use core::ops::Range;

use crate::part::tree_id::TreeId;

/// One node: a part (authored, or a generated copy) or a generated segment.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    pub id: TreeId,
    /// Its parent's index in [`crate::part::Tree::nodes`]; `None` at the top.
    pub parent: Option<usize>,
    /// One past the last node of its subtree: the subtree is
    /// `nodes[index..end]`, depth first.
    pub end: usize,
    /// The segment it is, when it is one: a leaf. A shape that renders
    /// exactly one segment IS that segment (a path and its one segment are
    /// one row).
    pub segment: Option<usize>,
    /// The segments under it (itself included), as a range of
    /// [`crate::part::Tree::segments`]: in tree order, a subtree's segments
    /// are contiguous.
    pub segments: Range<usize>,
}

/// What made a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A part as authored (copy 0 of anything in a repeat is authored too:
    /// it carries the original's ids).
    AuthoredPart,
    /// A repeat's copy k ≥ 1 and every part in it.
    GeneratedPart,
    /// A segment a part generated: a circle's ring, a split path's subpath
    /// (in any copy).
    GeneratedSegment,
}

impl TreeNode {
    /// What made it, read from its id: a key means its part generated it;
    /// a copy means a repeat did.
    #[must_use]
    pub fn kind(&self) -> NodeKind {
        if self.id.key.is_some() {
            NodeKind::GeneratedSegment
        } else if self.id.copies.is_empty() {
            NodeKind::AuthoredPart
        } else {
            NodeKind::GeneratedPart
        }
    }

    /// Does it hold lamps of its own?
    #[must_use]
    pub fn is_segment(&self) -> bool {
        self.segment.is_some()
    }
}
