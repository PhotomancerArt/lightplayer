use std::collections::BTreeMap;

use lpc_model::{
    ArtifactPath, ArtifactPathSlot, EnumSlot, MapSlot, OptionSlot, Slotted, ValueSlot,
};

#[derive(Default, Slotted)]
pub struct ProjectDef {
    pub name: OptionSlot<ValueSlot<String>>,
    pub nodes: MapSlot<String, NodeInvocationDef>,
}

#[derive(Default, Slotted)]
pub struct NodeInvocationDef {
    pub def: EnumSlot<NodeDefRef>,
}

#[derive(Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
pub enum NodeDefRef {
    Path(ArtifactPathSlot),
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
            name: OptionSlot::some(ValueSlot::new("basic".to_string())),
            nodes: MapSlot::new(nodes),
        }
    }
}

impl NodeInvocationDef {
    pub fn new(path: &str) -> Self {
        Self {
            def: EnumSlot::new(NodeDefRef::Path(ArtifactPathSlot::new(ArtifactPath(
                path.to_string(),
            )))),
        }
    }

    pub fn def_path(&self) -> &str {
        match self.def.value() {
            NodeDefRef::Path(path) => path.value().as_str(),
        }
    }
}
