use lpc_model::Revision;

use crate::ProjectSlotAddress;

/// Compact shape/value family for a project slot descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectSlotDescriptorKind {
    Unit,
    Value,
    Record,
    Map,
    Enum,
    Option,
    Asset,
    Issue,
}

/// Latest mirror-derived slot data used to reconcile a slot controller.
///
/// M2 keeps this intentionally compact. M3 will project real `SlotData` and
/// `SlotShapeView` values into richer `UiConfigSlot` DTOs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSlotDescriptor {
    pub address: ProjectSlotAddress,
    pub label: String,
    pub kind: ProjectSlotDescriptorKind,
    pub revision: Option<Revision>,
    pub children: Vec<ProjectSlotDescriptor>,
}

impl ProjectSlotDescriptor {
    /// Create a descriptor without revision or children.
    pub fn new(
        address: ProjectSlotAddress,
        label: impl Into<String>,
        kind: ProjectSlotDescriptorKind,
    ) -> Self {
        Self {
            address,
            label: label.into(),
            kind,
            revision: None,
            children: Vec::new(),
        }
    }

    /// Set the latest known revision for this slot.
    pub fn with_revision(mut self, revision: Revision) -> Self {
        self.revision = Some(revision);
        self
    }

    /// Set child slot descriptors.
    pub fn with_children(mut self, children: Vec<ProjectSlotDescriptor>) -> Self {
        self.children = children;
        self
    }

    /// Stable controller address for this slot.
    pub fn address(&self) -> &ProjectSlotAddress {
        &self.address
    }
}
