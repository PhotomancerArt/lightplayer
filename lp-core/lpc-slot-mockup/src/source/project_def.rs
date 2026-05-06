use std::collections::BTreeMap;

use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotMap, SlotMapKeyShape, SlotMapValueAccess,
    SlotRecordAccess, SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError, SlotValue,
    StaticSlotAccess,
};

use crate::model::{field, id, map, record, reference, value};

pub struct ProjectDef {
    nodes: SlotMap<String, NodeInvocationDef>,
}

pub struct NodeInvocationDef {
    artifact: SlotValue<String>,
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

impl SlotAccess for ProjectDef {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for ProjectDef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.project");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        registry.register_tree(
            id("source.node_invocation"),
            record(vec![field("artifact", value(ModelType::String))]),
        )?;

        registry.register_tree(
            Self::SHAPE_ID,
            record(vec![field(
                "nodes",
                map(
                    SlotMapKeyShape::String,
                    reference(id("source.node_invocation")),
                ),
            )]),
        )
    }
}

impl SlotRecordAccess for ProjectDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Map(&self.nodes)),
            _ => None,
        }
    }
}

impl NodeInvocationDef {
    fn new(artifact: &str) -> Self {
        Self {
            artifact: SlotValue::new(artifact.to_string()),
        }
    }
}

impl SlotMapValueAccess for NodeInvocationDef {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for NodeInvocationDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.artifact)),
            _ => None,
        }
    }
}
