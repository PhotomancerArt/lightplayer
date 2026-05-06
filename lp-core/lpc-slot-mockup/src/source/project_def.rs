use std::collections::BTreeMap;

use lpc_model::{ArtifactPathSlot, MapSlot};

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct ProjectDef {
    nodes: MapSlot<String, NodeInvocationDef>,
}

#[derive(lpc_model::SlotRecord)]
pub struct NodeInvocationDef {
    artifact: ArtifactPathSlot,
}

impl ProjectDef {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            String::from("output"),
            NodeInvocationDef::new("./output.toml"),
        );
        nodes.insert(
            String::from("texture"),
            NodeInvocationDef::new("./texture.toml"),
        );
        nodes.insert(
            String::from("fixture"),
            NodeInvocationDef::new("./fixture.toml"),
        );
        nodes.insert(
            String::from("shader"),
            NodeInvocationDef::new("./shader.toml"),
        );

        Self {
            nodes: MapSlot::new(nodes),
        }
    }
}

impl Default for ProjectDef {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeInvocationDef {
    fn new(artifact: &str) -> Self {
        Self {
            artifact: ArtifactPathSlot::new(artifact.to_string()),
        }
    }
}
