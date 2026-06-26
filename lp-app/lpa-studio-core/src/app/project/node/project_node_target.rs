use lpc_model::NodeId;

use super::ProjectNodeAddress;

/// Runtime action target for a project node.
///
/// The `address` is the stable controller key. The `node_id` is the current
/// server/runtime handle carried on action targets for efficient dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeTarget {
    pub address: ProjectNodeAddress,
    pub node_id: NodeId,
}

impl ProjectNodeTarget {
    /// Create a node target from a stable address and current runtime id.
    pub fn new(address: ProjectNodeAddress, node_id: NodeId) -> Self {
        Self { address, node_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_keeps_address_and_runtime_id_separate() {
        let address = ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap();
        let target = ProjectNodeTarget::new(address.clone(), NodeId::new(7));

        assert_eq!(target.address, address);
        assert_eq!(target.node_id, NodeId::new(7));
    }
}
