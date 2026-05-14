use std::collections::BTreeMap;

use lpc_model::{
    AddSubMode, BindingDefs, DivMode, GlslOpts, LpValue, MapSlot, MulMode, OptionSlot, PositiveF32,
    PositiveF32Slot, Ratio, RatioSlot, RenderOrder, RenderOrderSlot, Revision, SlotRecord,
    SourcePath, SourcePathSlot, ValueSlot,
};

#[derive(SlotRecord)]
pub struct ShaderDef {
    pub glsl_path: SourcePathSlot,
    pub render_order: RenderOrderSlot,
    pub bindings: BindingDefs,
    pub glsl_opts: GlslOpts,
    pub param_defs: MapSlot<String, ShaderParamDef>,
}

#[derive(Clone, Debug, PartialEq, SlotRecord)]
pub struct ShaderParamDef {
    pub label: ValueSlot<String>,
    pub description: ValueSlot<String>,
    pub value_type: ValueSlot<String>,
    pub default: RatioSlot,
    pub min: OptionSlot<ScalarHint>,
}

#[derive(Clone, Debug, PartialEq, SlotRecord)]
pub struct ScalarHint {
    pub value: PositiveF32Slot,
}

impl ShaderDef {
    pub const KIND: &'static str = "shader";

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
            glsl_path: SourcePathSlot::new(SourcePath(String::from("main.glsl"))),
            render_order: RenderOrderSlot::new(RenderOrder(0)),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts {
                add_sub: ValueSlot::new(AddSubMode::Wrapping),
                mul: ValueSlot::new(MulMode::Wrapping),
                div: ValueSlot::new(DivMode::Reciprocal),
            },
            param_defs: MapSlot::new(param_defs),
        }
    }

    pub fn from_codec(
        glsl_path: String,
        render_order: i32,
        glsl_opts: GlslOpts,
        param_defs: BTreeMap<String, ShaderParamDef>,
    ) -> Self {
        Self {
            glsl_path: SourcePathSlot::new(SourcePath(glsl_path)),
            render_order: RenderOrderSlot::new(RenderOrder(render_order)),
            bindings: BindingDefs::default(),
            glsl_opts,
            param_defs: MapSlot::new(param_defs),
        }
    }

    pub fn glsl_path(&self) -> &str {
        self.glsl_path.value().as_str()
    }

    pub fn render_order(&self) -> i32 {
        self.render_order.value().0
    }

    pub fn glsl_opts(&self) -> &GlslOpts {
        &self.glsl_opts
    }

    pub fn bindings(&self) -> &BindingDefs {
        &self.bindings
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

impl ShaderParamDef {
    pub fn new(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            label: ValueSlot::new(label.to_string()),
            description: ValueSlot::new(description.to_string()),
            value_type: ValueSlot::new(String::from("f32")),
            default: RatioSlot::new(Ratio(default)),
            min: match min {
                Some(value) => OptionSlot::some(ScalarHint::new(value)),
                None => OptionSlot::none(),
            },
        }
    }

    pub fn default_value(&self) -> LpValue {
        LpValue::F32(self.default.value().0)
    }

    pub fn label(&self) -> &str {
        self.label.value()
    }

    pub fn description(&self) -> &str {
        self.description.value()
    }

    pub fn value_type(&self) -> &str {
        self.value_type.value()
    }

    pub fn default_scalar(&self) -> f32 {
        self.default.value().0
    }

    pub fn min(&self) -> Option<&ScalarHint> {
        self.min.data.as_ref()
    }

    fn set_value_type(&mut self, value_type: &str) {
        self.value_type.set(value_type.to_string());
    }

    pub fn set_value_type_for_codec(&mut self, value_type: &str) {
        self.set_value_type(value_type);
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
    pub fn new(value: f32) -> Self {
        Self {
            value: PositiveF32Slot::new(PositiveF32(value)),
        }
    }

    pub fn mock(value: f32) -> Self {
        Self::new(value)
    }

    pub fn value(&self) -> f32 {
        self.value.value().0
    }
}
