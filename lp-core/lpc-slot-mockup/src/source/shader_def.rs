use std::collections::BTreeMap;

use lpc_model::{
    LpValue, MapSlot, OptionSlot, PositiveF32Slot, RatioSlot, RelativeNodeRef, RelativeNodeRefSlot,
    RenderOrderSlot, Revision, SourcePathSlot, ValueSlot,
};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct ShaderDef {
    glsl_path: SourcePathSlot,
    texture_loc: RelativeNodeRefSlot,
    render_order: RenderOrderSlot,
    compiler_options: CompilerOptions,
    pub param_defs: MapSlot<String, ShaderParamDef>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
pub struct CompilerOptions {
    add_sub: ValueSlot<String>,
    mul: ValueSlot<String>,
    div: ValueSlot<String>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
pub struct ShaderParamDef {
    label: ValueSlot<String>,
    description: ValueSlot<String>,
    value_type: ValueSlot<String>,
    default: RatioSlot,
    min: OptionSlot<ScalarHint>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
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
            param_defs: MapSlot::new(param_defs),
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

    pub fn param_label_revision(&self, name: &str) -> Option<Revision> {
        self.param_defs
            .entries
            .get(name)
            .map(ShaderParamDef::label_revision)
    }

    pub fn param_default_revision(&self, name: &str) -> Option<Revision> {
        self.param_defs
            .entries
            .get(name)
            .map(ShaderParamDef::default_revision)
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
            add_sub: ValueSlot::new(String::from("saturating")),
            mul: ValueSlot::new(String::from("saturating")),
            div: ValueSlot::new(String::from("saturating")),
        }
    }
}

impl ShaderParamDef {
    fn new(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            label: ValueSlot::new(label.to_string()),
            description: ValueSlot::new(description.to_string()),
            value_type: ValueSlot::new(String::from("f32")),
            default: RatioSlot::new(default),
            min: match min {
                Some(value) => OptionSlot::some(ScalarHint::new(value)),
                None => OptionSlot::none(),
            },
        }
    }

    pub fn default_value(&self) -> LpValue {
        LpValue::F32(*self.default.value())
    }

    fn set_value_type(&mut self, value_type: &str) {
        self.value_type.set(value_type.to_string());
    }

    fn set_label(&mut self, label: &str) {
        self.label.set(label.to_string());
    }

    fn label_revision(&self) -> Revision {
        self.label.revision()
    }

    fn default_revision(&self) -> Revision {
        self.default.revision()
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
