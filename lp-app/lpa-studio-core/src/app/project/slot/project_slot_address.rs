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

    /// True when this address lies **strictly under** `ancestor`: same node
    /// and slot root, with `ancestor.path` as a proper prefix of this path.
    /// An address is never strictly under itself.
    pub fn is_strictly_under(&self, ancestor: &ProjectSlotAddress) -> bool {
        self.node == ancestor.node
            && self.root == ancestor.root
            && self.path.segments().len() > ancestor.path.segments().len()
            && self.path.segments().starts_with(ancestor.path.segments())
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

    #[test]
    fn strictly_under_requires_same_node_root_and_proper_path_prefix() {
        let node = ProjectNodeAddress::parse("/demo.project/pixels.fixture").unwrap();
        let other_node = ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap();
        let at = |path: &str| {
            ProjectSlotAddress::new(
                node.clone(),
                ProjectSlotRoot::def(),
                SlotPath::parse(path).unwrap(),
            )
        };

        let map = at("mapping.PathPoints.paths");
        let entry = at("mapping.PathPoints.paths[0]");
        let nested = at("mapping.PathPoints.paths[0].RingArray.diameter");

        assert!(entry.is_strictly_under(&map));
        assert!(nested.is_strictly_under(&map));
        assert!(nested.is_strictly_under(&entry));
        assert!(entry.is_strictly_under(&ProjectSlotAddress::root(
            node.clone(),
            ProjectSlotRoot::def()
        )));

        assert!(!map.is_strictly_under(&map), "never under itself");
        assert!(!map.is_strictly_under(&entry), "prefix is directional");
        assert!(
            !at("mapping.PathPointsExtra").is_strictly_under(&at("mapping.PathPoints")),
            "segment-wise prefix, not string prefix"
        );
        assert!(
            !ProjectSlotAddress::new(
                other_node,
                ProjectSlotRoot::def(),
                SlotPath::parse("mapping.PathPoints.paths[0]").unwrap(),
            )
            .is_strictly_under(&map),
            "different node never matches"
        );
        assert!(
            !ProjectSlotAddress::new(
                node,
                ProjectSlotRoot::state(),
                SlotPath::parse("mapping.PathPoints.paths[0]").unwrap(),
            )
            .is_strictly_under(&map),
            "different slot root never matches"
        );
    }
}
