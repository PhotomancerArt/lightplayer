use crate::ProjectNodeDescriptor;

/// Desired project editor controller tree for one reconciliation pass.
///
/// M2 builds this in tests. M3 will project real `ProjectView` data into this
/// descriptor before rendering node DTOs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectEditorTreeDescriptor {
    pub nodes: Vec<ProjectNodeDescriptor>,
}

impl ProjectEditorTreeDescriptor {
    /// Create a descriptor from nodes in desired display order.
    pub fn new(nodes: Vec<ProjectNodeDescriptor>) -> Self {
        Self { nodes }
    }

    /// True when no nodes are described.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
