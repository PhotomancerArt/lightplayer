use std::collections::BTreeMap;

use lpc_model::{
    ModelType, ModelValue, SlotAccess, SlotData, SlotDataAccess, SlotMapDyn, SlotMapKey,
    SlotMapKeyShape, SlotOptionDyn, SlotRecordAccess, SlotShapeChild, SlotShapeId,
    SlotShapeRegistry, SlotShapeRegistryError, StaticSlotAccess, Versioned, current_state_version,
};

use crate::model::{field, id, map, option, record, value, version};
use crate::source::ShaderDef;

pub struct ShaderNode {
    params: SlotMapDyn,
    compile_error: SlotOptionDyn,
}

impl ShaderNode {
    pub fn from_def(def: &ShaderDef) -> Self {
        let entries = def
            .param_defs
            .entries
            .iter()
            .map(|(name, param_def)| {
                (
                    SlotMapKey::String(name.clone()),
                    SlotData::Value(Versioned::new(
                        current_state_version(),
                        param_def.default_value(),
                    )),
                )
            })
            .collect::<BTreeMap<_, _>>();

        Self {
            params: SlotMapDyn::new(entries),
            compile_error: SlotOptionDyn::some_with_version(
                current_state_version(),
                SlotData::Value(Versioned::new(
                    current_state_version(),
                    ModelValue::String(String::from("initial compile warning")),
                )),
            ),
        }
    }
    pub fn set_param(&mut self, name: &str, value: f32) {
        let key = SlotMapKey::String(name.to_string());
        let Some(SlotData::Value(param)) = self.params.entries.get_mut(&key) else {
            panic!("shader param exists");
        };
        param.set(current_state_version(), ModelValue::F32(value));
    }

    pub fn remove_param(&mut self, name: &str) {
        if self
            .params
            .entries
            .remove(&SlotMapKey::String(name.to_string()))
            .is_some()
        {
            self.params.keys_changed_frame = current_state_version();
        }
    }

    pub fn clear_compile_error(&mut self) {
        self.compile_error = SlotOptionDyn::none();
    }
}

impl SlotAccess for ShaderNode {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for ShaderNode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("engine.shader_node");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        use SlotShapeChild::{Owned, Ref};

        registry.register_tree(
            version(),
            id("engine.shader_param_value"),
            vec![value("engine.shader_param_value", ModelType::F32)],
        )?;

        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            vec![
                record(
                    "engine.shader_node",
                    vec![
                        field("params", Owned(id("engine.shader_node.params"))),
                        field(
                            "compile_error",
                            Owned(id("engine.shader_node.compile_error")),
                        ),
                    ],
                ),
                map(
                    "engine.shader_node.params",
                    SlotMapKeyShape::String,
                    Ref(id("engine.shader_param_value")),
                ),
                option(
                    "engine.shader_node.compile_error",
                    Owned(id("engine.shader_node.compile_error.value")),
                ),
                value("engine.shader_node.compile_error.value", ModelType::String),
            ],
        )
    }
}

impl SlotRecordAccess for ShaderNode {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Map(&self.params)),
            1 => Some(SlotDataAccess::Option(&self.compile_error)),
            _ => None,
        }
    }
}
