//! The rendered tree: what a fixture's parts render, as one tree whose
//! leaves are segments.
//!
//! One node per part; a group's node holds its parts' nodes; a repeat's node
//! holds its copies (copy 0 is the repeated part's own node, copy k the same
//! subtree in copy k); a circle's node holds its ring segments; a split
//! path's node holds its subpath segments. A shape that renders exactly one
//! segment IS that segment.
//!
//! **Memory, as a device needs it.** The nodes are one array, depth first
//! (a subtree is a contiguous run of it), and every lamp position is in one
//! exact-size buffer, each segment a range of it: no per-lamp or
//! per-segment allocation of positions ([`crate::resolve_into`]'s rule).
//!
//! **Lamp order:** the segments in tree order are the order the fixture
//! lists its lamps.

use alloc::vec::Vec;

use crate::part::part_id::PartId;
use crate::part::segment::Segment;
use crate::part::tree_id::TreeId;
use crate::part::tree_node::TreeNode;

/// A rendered mapping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    pub(crate) nodes: Vec<TreeNode>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) positions: Vec<[f32; 2]>,
}

impl Tree {
    /// Every node, depth first.
    #[must_use]
    pub fn nodes(&self) -> &[TreeNode] {
        &self.nodes
    }

    /// Every segment, in tree order: the fixture's lamp order.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Every lamp's position, in the fixture's lamp order.
    #[must_use]
    pub fn positions(&self) -> &[[f32; 2]] {
        &self.positions
    }

    /// Move every lamp's position out, leaving the tree without them: the
    /// engine keeps them once, in its own carrier, and the tree (nodes and
    /// segments, which patching reads) without a second copy. Afterwards
    /// [`Tree::positions`] is empty and [`Tree::lamps_of`] must not be
    /// called.
    pub fn take_positions(&mut self) -> Vec<[f32; 2]> {
        core::mem::take(&mut self.positions)
    }

    /// One segment's lamp positions, lamp 0 first.
    #[must_use]
    pub fn lamps_of(&self, segment: &Segment) -> &[[f32; 2]] {
        &self.positions[segment.lamps.start as usize..segment.lamps.end as usize]
    }

    /// The node with this id.
    #[must_use]
    pub fn find(&self, id: &TreeId) -> Option<usize> {
        self.nodes.iter().position(|n| &n.id == id)
    }

    /// The node at `index`.
    #[must_use]
    pub fn node(&self, index: usize) -> &TreeNode {
        &self.nodes[index]
    }

    /// The segment with this id.
    #[must_use]
    pub fn segment(&self, id: &TreeId) -> Option<&Segment> {
        self.segments.iter().find(|s| &s.id == id)
    }

    /// A node's parent.
    #[must_use]
    pub fn parent(&self, index: usize) -> Option<usize> {
        self.nodes[index].parent
    }

    /// A node's children, in order.
    pub fn children(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let end = self.nodes[index].end;
        let mut next = index + 1;
        core::iter::from_fn(move || {
            (next < end).then(|| {
                let child = next;
                next = self.nodes[child].end;
                child
            })
        })
    }

    /// The top-level nodes (the mapping's parts), in order.
    pub fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        let mut next = 0;
        core::iter::from_fn(move || {
            (next < self.nodes.len()).then(|| {
                let root = next;
                next = self.nodes[root].end;
                root
            })
        })
    }

    /// The segments under a node (itself, when it is one), in tree order.
    #[must_use]
    pub fn segments_under(&self, index: usize) -> &[Segment] {
        &self.segments[self.nodes[index].segments.clone()]
    }

    /// The authored part a node came from (a copy's part is the original's).
    #[must_use]
    pub fn part_of(&self, index: usize) -> &PartId {
        &self.nodes[index].id.part
    }
}
