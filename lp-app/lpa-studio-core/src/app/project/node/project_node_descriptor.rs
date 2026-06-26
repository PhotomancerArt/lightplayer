use crate::{ProjectNodeStatusView, ProjectSlotDescriptor};

use super::{ProjectNodeAddress, ProjectNodeTarget};

/// Latest mirror-derived node data used to reconcile a node controller.
///
/// This is intentionally not a render DTO. It is the desired controller shape:
/// stable address, current runtime target, tree relationships, status, and root
/// slot descriptors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeDescriptor {
    pub target: ProjectNodeTarget,
    pub parent: Option<ProjectNodeAddress>,
    pub children: Vec<ProjectNodeAddress>,
    pub label: String,
    pub kind: String,
    pub status: ProjectNodeStatusView,
    pub slots: Vec<ProjectSlotDescriptor>,
}

impl ProjectNodeDescriptor {
    /// Create a descriptor with no child nodes or slots.
    pub fn new(
        target: ProjectNodeTarget,
        label: impl Into<String>,
        kind: impl Into<String>,
        status: ProjectNodeStatusView,
    ) -> Self {
        Self {
            target,
            parent: None,
            children: Vec::new(),
            label: label.into(),
            kind: kind.into(),
            status,
            slots: Vec::new(),
        }
    }

    /// Set the parent node address.
    pub fn with_parent(mut self, parent: ProjectNodeAddress) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Set child node addresses in display/order-preserving order.
    pub fn with_children(mut self, children: Vec<ProjectNodeAddress>) -> Self {
        self.children = children;
        self
    }

    /// Set root slot descriptors.
    pub fn with_slots(mut self, slots: Vec<ProjectSlotDescriptor>) -> Self {
        self.slots = slots;
        self
    }

    /// Stable controller address for this node.
    pub fn address(&self) -> &ProjectNodeAddress {
        &self.target.address
    }
}
