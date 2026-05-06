use std::collections::BTreeMap;

use lpc_model::{
    ModelType, ModelValue, RelativeNodeRef, SlotAccess, SlotDataAccess, SlotMap, SlotMapKeyShape,
    SlotMapValueAccess, SlotOption, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, SlotValue, StaticSlotAccess,
};

use crate::model::{field, id, map, option, record, reference, value, version};

pub struct ShaderDef {
    glsl_path: SlotValue<String>,
    texture_loc: SlotValue<RelativeNodeRef>,
    render_order: SlotValue<i32>,
    compiler_options: CompilerOptions,
    pub param_defs: SlotMap<String, ShaderParamDef>,
}

pub struct CompilerOptions {
    add_sub: SlotValue<String>,
    mul: SlotValue<String>,
    div: SlotValue<String>,
}

pub struct ShaderParamDef {
    label: SlotValue<String>,
    description: SlotValue<String>,
    value_type: SlotValue<String>,
    default: SlotValue<f32>,
    min: SlotOption<ScalarHint>,
}

pub struct ScalarHint {
    value: SlotValue<f32>,
}

impl ShaderDef {
    pub fn new() -> Self {
        let mut param_defs = BTreeMap::new();
        param_defs.insert(
            String::from("exposure"),
            ShaderParamDef::new("Exposure", "Output exposure multiplier", 1.0, Some(0.0)),
        );
        param_defs.insert(
            String::from("speed"),
            ShaderParamDef::new("Speed", "Animation speed", 0.25, Some(0.0)),
        );

        Self {
            glsl_path: SlotValue::new(String::from("shader.glsl")),
            texture_loc: SlotValue::new(RelativeNodeRef::parse("..texture").unwrap()),
            render_order: SlotValue::new(0),
            compiler_options: CompilerOptions::default(),
            param_defs: SlotMap::new(param_defs),
        }
    }

    pub fn add_param_def(&mut self, name: &str, default: f32) {
        self.param_defs.insert(
            name.to_string(),
            ShaderParamDef::new(name, "Dynamically authored shader parameter", default, None),
        );
    }
}

impl Default for ShaderDef {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotAccess for ShaderDef {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for ShaderDef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.shader");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        registry.register_tree(
            version(),
            id("source.scalar_hint"),
            record(vec![field("value", value(ModelType::F32))]),
        )?;

        registry.register_tree(
            version(),
            id("source.shader_param_def"),
            record(vec![
                field("label", value(ModelType::String)),
                field("description", value(ModelType::String)),
                field("value_type", value(ModelType::String)),
                field("default", value(ModelType::F32)),
                field("min", option(reference(id("source.scalar_hint")))),
            ]),
        )?;

        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            record(vec![
                field("glsl_path", value(ModelType::String)),
                field("texture_loc", value(ModelType::String)),
                field("render_order", value(ModelType::I32)),
                field(
                    "compiler_options",
                    record(vec![
                        field("add_sub", value(ModelType::String)),
                        field("mul", value(ModelType::String)),
                        field("div", value(ModelType::String)),
                    ]),
                ),
                field(
                    "param_defs",
                    map(
                        SlotMapKeyShape::String,
                        reference(id("source.shader_param_def")),
                    ),
                ),
            ]),
        )
    }
}

impl SlotRecordAccess for ShaderDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.glsl_path)),
            1 => Some(SlotDataAccess::Value(&self.texture_loc)),
            2 => Some(SlotDataAccess::Value(&self.render_order)),
            3 => Some(SlotDataAccess::Record(&self.compiler_options)),
            4 => Some(SlotDataAccess::Map(&self.param_defs)),
            _ => None,
        }
    }
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            add_sub: SlotValue::new(String::from("saturating")),
            mul: SlotValue::new(String::from("saturating")),
            div: SlotValue::new(String::from("saturating")),
        }
    }
}

impl SlotRecordAccess for CompilerOptions {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.add_sub)),
            1 => Some(SlotDataAccess::Value(&self.mul)),
            2 => Some(SlotDataAccess::Value(&self.div)),
            _ => None,
        }
    }
}

impl ShaderParamDef {
    fn new(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            label: SlotValue::new(label.to_string()),
            description: SlotValue::new(description.to_string()),
            value_type: SlotValue::new(String::from("f32")),
            default: SlotValue::new(default),
            min: match min {
                Some(value) => SlotOption::some(ScalarHint::new(value)),
                None => SlotOption::none(),
            },
        }
    }

    pub fn default_value(&self) -> ModelValue {
        ModelValue::F32(*self.default.value())
    }
}

impl SlotMapValueAccess for ShaderParamDef {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for ShaderParamDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.label)),
            1 => Some(SlotDataAccess::Value(&self.description)),
            2 => Some(SlotDataAccess::Value(&self.value_type)),
            3 => Some(SlotDataAccess::Value(&self.default)),
            4 => Some(SlotDataAccess::Option(&self.min)),
            _ => None,
        }
    }
}

impl ScalarHint {
    fn new(value: f32) -> Self {
        Self {
            value: SlotValue::new(value),
        }
    }

    pub fn mock(value: f32) -> Self {
        Self::new(value)
    }
}

impl SlotMapValueAccess for ScalarHint {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for ScalarHint {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.value)),
            _ => None,
        }
    }
}
