//! Where a part, or any node of a fixture's rendered tree, lives in the
//! project: its fixture and its id there.
//!
//! A mapping's ids are its own (`lpc_mapping::part`, names.html "Ids"):
//! flat and never reused inside one fixture, so two fixtures may both have
//! a `path1`. The project's selection, its board and its patch reach across
//! fixtures, so they name a part or a node with its fixture.

use lpc_mapping::part::{PartId, TreeId};

use crate::domain::node::NodeId;

/// An authored part, in its fixture: `circle1` of `display`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartRef {
    pub fixture: NodeId,
    pub part: PartId,
}

/// A node of a fixture's rendered tree, in its fixture: a part's own node
/// (in any copy), a ring, a subpath, a copy (`circle1/r3`, `panel@2`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TreeRef {
    pub fixture: NodeId,
    pub id: TreeId,
}

impl PartRef {
    pub fn new(fixture: &NodeId, part: &PartId) -> Self {
        Self {
            fixture: fixture.clone(),
            part: part.clone(),
        }
    }

    /// Read `fixture` and a part id's text; `None` when the text isn't one.
    pub fn parse(fixture: &NodeId, part: &str) -> Option<Self> {
        Some(Self::new(fixture, &PartId::new(part).ok()?))
    }

    /// The part's own node, in copy 0.
    pub fn node(&self) -> TreeRef {
        TreeRef {
            fixture: self.fixture.clone(),
            id: TreeId::of(&self.part),
        }
    }
}

impl TreeRef {
    pub fn new(fixture: &NodeId, id: TreeId) -> Self {
        Self {
            fixture: fixture.clone(),
            id,
        }
    }

    /// Read `fixture` and a node id's text (`circle1/r3@2`); `None` when
    /// the text isn't one.
    pub fn parse(fixture: &NodeId, id: &str) -> Option<Self> {
        Some(Self::new(fixture, TreeId::parse(id)?))
    }

    /// The part that rendered it.
    pub fn part(&self) -> PartRef {
        PartRef::new(&self.fixture, &self.id.part)
    }

    /// Is it a part's own node in copy 0: the part as authored?
    pub fn is_part_node(&self) -> bool {
        self.id.key.is_none() && self.id.copies.is_empty()
    }

    /// The same node in copy 0.
    pub fn original(&self) -> Self {
        Self::new(&self.fixture, self.id.original())
    }
}

impl std::fmt::Display for PartRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.part)
    }
}

impl std::fmt::Display for TreeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}
