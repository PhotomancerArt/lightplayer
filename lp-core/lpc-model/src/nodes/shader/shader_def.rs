use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::nodes::shader::{GlslOpts, ShaderSlotDef};
use crate::{AsLpPathBuf, BindingDefs, LpPathBuf, MapSlot, RenderOrderSlot, SourcePathSlot};

/// Authored visual shader node definition.
///
/// Visual shaders produce a `VisualProduct` from GLSL pixel/sample code. Values
/// consumed by the shader are declared as authored slot definitions under
/// `consumed`; bindings decide where those values come from at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct ShaderDef {
    /// Path to the GLSL source, relative to this artifact file.
    pub glsl_path: SourcePathSlot,
    /// Render order - lower numbers render first (default 0)
    pub render_order: RenderOrderSlot,
    /// Authored slot bindings for shader inputs and outputs.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    /// GLSL compilation options
    #[serde(default)]
    pub glsl_opts: GlslOpts,
    /// Shader-consumed slots exposed to the resolver and GLSL uniform block.
    #[serde(
        default,
        rename = "consumed",
        skip_serializing_if = "MapSlot::is_empty"
    )]
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
}

impl Default for ShaderDef {
    fn default() -> Self {
        Self {
            glsl_path: SourcePathSlot::new(String::from("main.glsl")),
            render_order: RenderOrderSlot::new(0),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            consumed_slots: MapSlot::default(),
        }
    }
}

impl ShaderDef {
    pub const KIND: &'static str = "shader/visual";

    pub fn glsl_path_buf(&self) -> LpPathBuf {
        self.glsl_path.value().as_path_buf()
    }

    pub fn render_order(&self) -> i32 {
        *self.render_order.value()
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Shader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::shader::{AddSubMode, DivMode, MulMode};
    use crate::{NodeKind, ShaderDefView, SlotPath, SlotShapeRegistry, StaticSlotShape};

    #[test]
    fn test_shader_def_kind() {
        let def = ShaderDef {
            glsl_path: SourcePathSlot::new(String::from("main.glsl")),
            render_order: RenderOrderSlot::new(0),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            consumed_slots: MapSlot::default(),
        };
        assert_eq!(def.kind(), NodeKind::Shader);
    }

    #[test]
    fn test_shader_def_default() {
        let def = ShaderDef::default();
        assert_eq!(def.glsl_path.value(), "main.glsl");
        assert_eq!(def.render_order(), 0);
        assert_eq!(*def.glsl_opts.add_sub.value(), AddSubMode::Wrapping);
        assert_eq!(*def.glsl_opts.mul.value(), MulMode::Wrapping);
        assert_eq!(*def.glsl_opts.div.value(), DivMode::Reciprocal);
    }

    #[test]
    fn generated_shader_def_view_compiles() {
        let mut registry = SlotShapeRegistry::default();
        ShaderDef::ensure_registered(&mut registry).expect("shader shape");

        let view = ShaderDefView::compile(&registry).expect("shader def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(
            view.glsl_path().path(),
            &SlotPath::parse("glsl_path").unwrap()
        );
        assert_eq!(
            view.render_order().path(),
            &SlotPath::parse("render_order").unwrap()
        );
        assert_eq!(
            view.glsl_opts().path(),
            &SlotPath::parse("glsl_opts").unwrap()
        );
    }

    #[test]
    fn parses_visual_shader_consumed_slot() {
        let def: ShaderDef = toml::from_str(
            r#"
kind = "shader/visual"
glsl_path = "shader.glsl"
render_order = 0

[consumed.time]
kind = "value"
value = "f32"
default = 0.0
"#,
        )
        .expect("visual shader");

        assert_eq!(def.consumed_slots.entries.len(), 1);
        assert!(def.consumed_slots.entries.contains_key("time"));
    }
}
