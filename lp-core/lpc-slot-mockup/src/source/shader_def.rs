use std::collections::BTreeMap;

use lpc_model::{
    FrameId, ModelType, ModelValue, PositiveF32Slot, RatioSlot, RelativeNodeRef,
    RelativeNodeRefSlot, RenderOrderSlot, SlotAccess, SlotDataAccess, SlotMap, SlotMapKeyShape,
    SlotMapValueAccess, SlotOption, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, SlotValue, SourcePathSlot, StaticSlotAccess, positive_f32_shape,
    ratio_shape, relative_node_ref_shape, render_order_shape, source_path_shape,
};

use crate::model::{field, id, leaf, map, option, record, reference, value};

pub struct ShaderDef {
    glsl_path: SourcePathSlot,
    texture_loc: RelativeNodeRefSlot,
    render_order: RenderOrderSlot,
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
    default: RatioSlot,
    min: SlotOption<ScalarHint>,
}

pub struct ScalarHint {
    value: PositiveF32Slot,
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
            glsl_path: SourcePathSlot::new(String::from("shader.glsl")),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            render_order: RenderOrderSlot::new(0),
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

    pub fn set_param_value_type(&mut self, name: &str, value_type: &str) {
        let param = self.param_defs.entries.get_mut(name).expect("param def");
        param.set_value_type(value_type);
    }

    pub fn set_param_label(&mut self, name: &str, label: &str) {
        let param = self.param_defs.entries.get_mut(name).expect("param def");
        param.set_label(label);
    }

    pub fn param_label_changed_frame(&self, name: &str) -> Option<FrameId> {
        self.param_defs
            .entries
            .get(name)
            .map(ShaderParamDef::label_changed_frame)
    }

    pub fn param_default_changed_frame(&self, name: &str) -> Option<FrameId> {
        self.param_defs
            .entries
            .get(name)
            .map(ShaderParamDef::default_changed_frame)
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
            id("source.scalar_hint"),
            record(vec![field("value", leaf(positive_f32_shape()))]),
        )?;

        registry.register_tree(
            id("source.shader_param_def"),
            record(vec![
                field("label", value(ModelType::String)),
                field("description", value(ModelType::String)),
                field("value_type", value(ModelType::String)),
                field("default", leaf(ratio_shape())),
                field("min", option(reference(id("source.scalar_hint")))),
            ]),
        )?;

        registry.register_tree(
            Self::SHAPE_ID,
            record(vec![
                field("glsl_path", leaf(source_path_shape())),
                field("texture_loc", leaf(relative_node_ref_shape())),
                field("render_order", leaf(render_order_shape())),
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
            default: RatioSlot::new(default),
            min: match min {
                Some(value) => SlotOption::some(ScalarHint::new(value)),
                None => SlotOption::none(),
            },
        }
    }

    pub fn default_value(&self) -> ModelValue {
        ModelValue::F32(*self.default.value())
    }

    fn set_value_type(&mut self, value_type: &str) {
        self.value_type.set(value_type.to_string());
    }

    fn set_label(&mut self, label: &str) {
        self.label.set(label.to_string());
    }

    fn label_changed_frame(&self) -> FrameId {
        self.label.changed_frame()
    }

    fn default_changed_frame(&self) -> FrameId {
        self.default.changed_frame()
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
            value: PositiveF32Slot::new(value),
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
