use lpc_model::SlotPath;

use crate::{ProjectNodeAddress, ProjectSlotRoot};

/// Stable address for any node-owned slot, including root/container slots.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectSlotAddress {
    pub node: ProjectNodeAddress,
    pub root: ProjectSlotRoot,
    pub path: SlotPath,
}

impl ProjectSlotAddress {
    /// Create a slot address from a node, root, and path.
    pub fn new(node: ProjectNodeAddress, root: ProjectSlotRoot, path: SlotPath) -> Self {
        Self { node, root, path }
    }

    /// Create an address for a slot root itself.
    pub fn root(node: ProjectNodeAddress, root: ProjectSlotRoot) -> Self {
        Self::new(node, root, SlotPath::root())
    }

    /// True when this address points at the slot root rather than a child path.
    pub fn is_root(&self) -> bool {
        self.path.is_root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_slot_address_uses_root_path() {
        let address = ProjectSlotAddress::root(
            ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap(),
            ProjectSlotRoot::def(),
        );

        assert!(address.is_root());
        assert_eq!(address.root.name(), "def");
    }
}
