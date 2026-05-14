use std::collections::BTreeMap;

use lpc_model::{ArtifactPathSlot, MapSlot, OptionSlot, SlotRecord, ValueSlot};

#[derive(SlotRecord)]
pub struct ProjectDef {
    #[slot(skip)]
    pub kind: String,
    pub name: OptionSlot<ValueSlot<String>>,
    pub nodes: MapSlot<String, NodeInvocationDef>,
}

#[derive(SlotRecord)]
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
