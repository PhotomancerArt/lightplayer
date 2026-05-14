use std::collections::BTreeMap;

use lpc_model::{ArtifactPathSlot, MapSlot, OptionSlot, ValueSlot};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct ProjectDef {
    #[slot(skip)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "OptionSlot::is_none")]
    pub name: OptionSlot<ValueSlot<String>>,
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub nodes: MapSlot<String, NodeInvocationDef>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
pub struct NodeInvocationDef {
    artifact: ArtifactPathSlot,
}

impl ProjectDef {
    pub const KIND: &'static str = "project";

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
            kind: Self::KIND.to_string(),
            name: OptionSlot::some(ValueSlot::new("basic".to_string())),
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
    pub fn new(artifact: &str) -> Self {
        Self {
            artifact: ArtifactPathSlot::new(artifact.to_string()),
        }
    }

    pub fn artifact(&self) -> &str {
        self.artifact.value()
    }
}
