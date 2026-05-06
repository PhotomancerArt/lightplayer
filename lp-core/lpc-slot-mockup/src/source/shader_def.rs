use std::collections::BTreeMap;

use lpc_model::{
    FrameId, ModelType, ModelValue, PositiveF32Slot, RatioSlot, RelativeNodeRef,
    RelativeNodeRefSlot, RenderOrderSlot, SlotMap, SlotOption, SlotValue, SourcePathSlot,
    positive_f32_shape, ratio_shape, relative_node_ref_shape, render_order_shape,
    source_path_shape,
};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.shader")]
pub struct ShaderDef {
    #[slot(leaf = source_path_shape())]
    glsl_path: SourcePathSlot,
    #[slot(leaf = relative_node_ref_shape())]
    texture_loc: RelativeNodeRefSlot,
    #[slot(leaf = render_order_shape())]
    render_order: RenderOrderSlot,
    #[slot(record)]
    compiler_options: CompilerOptions,
    #[slot(map(key = "string", value_ref = "source.shader_param_def"))]
    pub param_defs: SlotMap<String, ShaderParamDef>,
}

#[derive(lpc_model::SlotRecord)]
pub struct CompilerOptions {
    #[slot(value = ModelType::String)]
    add_sub: SlotValue<String>,
    #[slot(value = ModelType::String)]
    mul: SlotValue<String>,
    #[slot(value = ModelType::String)]
    div: SlotValue<String>,
}

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.shader_param_def")]
pub struct ShaderParamDef {
    #[slot(value = ModelType::String)]
    label: SlotValue<String>,
    #[slot(value = ModelType::String)]
    description: SlotValue<String>,
    #[slot(value = ModelType::String)]
    value_type: SlotValue<String>,
    #[slot(leaf = ratio_shape())]
    default: RatioSlot,
    #[slot(option_ref = "source.scalar_hint")]
    min: SlotOption<ScalarHint>,
}

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "source.scalar_hint")]
pub struct ScalarHint {
    #[slot(leaf = positive_f32_shape())]
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

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            add_sub: SlotValue::new(String::from("saturating")),
            mul: SlotValue::new(String::from("saturating")),
            div: SlotValue::new(String::from("saturating")),
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
