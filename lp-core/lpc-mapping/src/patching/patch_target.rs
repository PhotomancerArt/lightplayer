//! What patching is said of: the fixture (its start and flow), a part (its
//! own start and flow, in every copy), or one node (its override).

use crate::part::{PartId, TreeId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchTarget {
    /// The fixture: its start, and how its top-level parts follow each
    /// other.
    Fixture,
    /// A part: its start applies to its node in every copy; its flow lays
    /// out its children.
    Part(PartId),
    /// One node, by its id: its override.
    Node(TreeId),
}
