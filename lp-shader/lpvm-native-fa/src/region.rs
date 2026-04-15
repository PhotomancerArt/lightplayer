//! Arena-based region tree for structured control flow (built in M4).

use alloc::vec::Vec;

/// Index into [`RegionTree::nodes`].
pub type RegionId = u16;

/// Invalid / unset region id.
pub const REGION_ID_NONE: RegionId = u16::MAX;

/// Structured region over a flat VInst slice (indices are instruction indices).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    /// Linear range `[start, end)` (half-open).
    Linear { start: u16, end: u16 },
    IfThenElse {
        head: RegionId,
        then_body: RegionId,
        else_body: RegionId,
        else_label: crate::vinst::LabelId,
        merge_label: crate::vinst::LabelId,
    },
    Loop {
        header: RegionId,
        body: RegionId,
        header_label: crate::vinst::LabelId,
        exit_label: crate::vinst::LabelId,
    },
    Seq {
        children_start: u16,
        child_count: u16,
    },
}

/// Arena of regions plus storage for [`Region::Seq`] child lists.
#[derive(Clone, Debug)]
pub struct RegionTree {
    pub nodes: Vec<Region>,
    pub seq_children: Vec<RegionId>,
    pub root: RegionId,
}

impl Default for RegionTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionTree {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Create with pre-allocated capacity for nodes.
    #[must_use]
    pub fn with_capacity(node_capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(node_capacity.saturating_add(4)),
            seq_children: Vec::new(),
            root: REGION_ID_NONE,
        }
    }

    /// Push a region into the arena, returning its [`RegionId`].
    #[must_use]
    pub fn push(&mut self, region: Region) -> RegionId {
        let id = self.nodes.len() as RegionId;
        self.nodes.push(region);
        id
    }

    /// Push a [`Region::Seq`] with the given children.
    #[must_use]
    pub fn push_seq(&mut self, children: &[RegionId]) -> RegionId {
        let start = self.seq_children.len() as u16;
        self.seq_children.extend_from_slice(children);
        self.push(Region::Seq {
            children_start: start,
            child_count: children.len() as u16,
        })
    }
}
