use std::collections::BTreeMap;

use lpc_model::{ArtifactPathSlot, SlotMap, artifact_path_shape};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.project")]
pub struct ProjectDef {
    #[slot(map(key = "string", value_ref = "source.node_invocation"))]
    nodes: SlotMap<String, NodeInvocationDef>,
}

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.node_invocation")]
pub struct NodeInvocationDef {
    #[slot(leaf = artifact_path_shape())]
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
            nodes: SlotMap::new(nodes),
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
