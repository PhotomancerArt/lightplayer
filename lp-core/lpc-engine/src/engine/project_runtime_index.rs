//! Projection index between project node uses and runtime node ids.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{AssetSource, NodeDefLocation, NodeId, NodeUseLocation};

/// Engine-local lookup table for the current registry-to-runtime projection.
///
/// `ProjectRegistry` owns project identity and effective inventory. The engine
/// owns compact runtime [`NodeId`] handles. This index connects those layers
/// without making either identity pretend to be the other.
#[derive(Debug, Default)]
pub struct ProjectRuntimeIndex {
    node_to_runtime: BTreeMap<NodeUseLocation, NodeId>,
    runtime_to_node: BTreeMap<NodeId, NodeUseLocation>,
    def_to_runtime: BTreeMap<NodeDefLocation, Vec<NodeId>>,
    asset_to_runtime: BTreeMap<AssetSource, Vec<NodeId>>,
}

impl ProjectRuntimeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_node(
        &mut self,
        use_location: NodeUseLocation,
        node_id: NodeId,
        def_location: NodeDefLocation,
    ) {
        self.node_to_runtime.insert(use_location.clone(), node_id);
        self.runtime_to_node.insert(node_id, use_location);
        self.def_to_runtime
            .entry(def_location)
            .or_default()
            .push(node_id);
    }

    pub fn add_asset_consumer(&mut self, source: AssetSource, node_id: NodeId) {
        self.asset_to_runtime
            .entry(source)
            .or_default()
            .push(node_id);
    }

    pub fn node_id(&self, use_location: &NodeUseLocation) -> Option<NodeId> {
        self.node_to_runtime.get(use_location).copied()
    }

    pub fn use_location(&self, node_id: NodeId) -> Option<&NodeUseLocation> {
        self.runtime_to_node.get(&node_id)
    }

    pub fn runtime_nodes_for_def(&self, location: &NodeDefLocation) -> &[NodeId] {
        self.def_to_runtime
            .get(location)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn runtime_nodes_for_asset(&self, source: &AssetSource) -> &[NodeId] {
        self.asset_to_runtime
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn clear(&mut self) {
        self.node_to_runtime.clear();
        self.runtime_to_node.clear();
        self.def_to_runtime.clear();
        self.asset_to_runtime.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{ArtifactLocation, SlotPath};

    fn def(path: &str) -> NodeDefLocation {
        NodeDefLocation::artifact_root(ArtifactLocation::file(path))
    }

    #[test]
    fn indexes_nodes_in_both_directions() {
        let mut index = ProjectRuntimeIndex::new();
        let use_location = NodeUseLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());
        let node_id = NodeId::new(7);

        index.insert_node(use_location.clone(), node_id, def("/shader.toml"));

        assert_eq!(index.node_id(&use_location), Some(node_id));
        assert_eq!(index.use_location(node_id), Some(&use_location));
    }

    #[test]
    fn definition_and_asset_indexes_allow_multiple_runtime_nodes() {
        let mut index = ProjectRuntimeIndex::new();
        let def_location = def("/shared.toml");
        let asset = AssetSource::artifact(ArtifactLocation::file("/shader.glsl"));

        index.insert_node(
            NodeUseLocation::root(),
            NodeId::new(1),
            def_location.clone(),
        );
        index.insert_node(
            NodeUseLocation::root().child(SlotPath::parse("nodes[copy]").unwrap()),
            NodeId::new(2),
            def_location.clone(),
        );
        index.add_asset_consumer(asset.clone(), NodeId::new(1));
        index.add_asset_consumer(asset.clone(), NodeId::new(2));

        assert_eq!(
            index.runtime_nodes_for_def(&def_location),
            &[NodeId::new(1), NodeId::new(2)]
        );
        assert_eq!(
            index.runtime_nodes_for_asset(&asset),
            &[NodeId::new(1), NodeId::new(2)]
        );
    }

    #[test]
    fn clear_empties_all_indexes() {
        let mut index = ProjectRuntimeIndex::new();
        let use_location = NodeUseLocation::root();
        let def_location = def("/project.toml");
        let asset = AssetSource::artifact(ArtifactLocation::file("/shader.glsl"));

        index.insert_node(use_location.clone(), NodeId::new(0), def_location.clone());
        index.add_asset_consumer(asset.clone(), NodeId::new(0));
        index.clear();

        assert_eq!(index.node_id(&use_location), None);
        assert!(index.runtime_nodes_for_def(&def_location).is_empty());
        assert!(index.runtime_nodes_for_asset(&asset).is_empty());
    }
}
