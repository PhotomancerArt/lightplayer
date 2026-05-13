use std::collections::BTreeMap;

use lpc_model::{
    MapSlot, RelativeNodeRef, RelativeNodeRefSlot, RenderOrderSlot, Revision, ShaderSlotDef,
    ShaderValueShapeRef, SourcePathSlot, ValueSlot,
};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct ShaderDef {
    glsl_path: SourcePathSlot,
    texture_loc: RelativeNodeRefSlot,
    render_order: RenderOrderSlot,
    compiler_options: CompilerOptions,
    #[serde(rename = "consumed")]
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
}

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
pub struct CompilerOptions {
    add_sub: ValueSlot<String>,
    mul: ValueSlot<String>,
    div: ValueSlot<String>,
}

impl ShaderDef {
    pub fn new() -> Self {
        let mut consumed_slots = BTreeMap::new();
        consumed_slots.insert(
            String::from("exposure"),
            ShaderSlotDef::value_f32("Exposure", "Output exposure multiplier", 1.0, Some(0.0)),
        );
        consumed_slots.insert(
            String::from("speed"),
            ShaderSlotDef::value_f32("Speed", "Animation speed", 0.25, Some(0.0)),
        );

        Self {
            glsl_path: SourcePathSlot::new(String::from("shader.glsl")),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            render_order: RenderOrderSlot::new(0),
            compiler_options: CompilerOptions::default(),
            consumed_slots: MapSlot::new(consumed_slots),
        }
    }

    pub fn add_consumed_slot(&mut self, name: &str, default: f32) {
        self.consumed_slots.insert(
            name.to_string(),
            ShaderSlotDef::value_f32(name, "Dynamically authored shader parameter", default, None),
        );
    }

    pub fn set_param_value_shape(&mut self, name: &str, value_shape: &str) {
        let param = self
            .consumed_slots
            .entries
            .get_mut(name)
            .expect("param def");
        param.value.set(ShaderValueShapeRef::builtin(value_shape));
    }

    pub fn set_param_label(&mut self, name: &str, label: &str) {
        let param = self
            .consumed_slots
            .entries
            .get_mut(name)
            .expect("param def");
        param.label.set(label.to_string());
    }

    pub fn param_label_revision(&self, name: &str) -> Option<Revision> {
        self.consumed_slots
            .entries
            .get(name)
            .map(|param| param.label.revision())
    }

    pub fn param_default_revision(&self, name: &str) -> Option<Revision> {
        self.consumed_slots
            .entries
            .get(name)
            .and_then(|param| param.default.data.as_ref().map(ValueSlot::revision))
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
