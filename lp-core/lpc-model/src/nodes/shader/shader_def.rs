use alloc::string::String;

use crate::nodes::shader::{GlslOpts, ShaderParamDef, ShaderSlotDef, ShaderSource};
use crate::{BindingDefs, EnumSlot, MapSlot, RenderOrderSlot, Slotted};

/// Authored shader node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ShaderDef {
    /// Authored shader source.
    pub source: EnumSlot<ShaderSource>,
    /// Render order - lower numbers render first (default 0)
    pub render_order: RenderOrderSlot,
    /// Authored slot bindings for shader inputs and outputs.
    pub bindings: BindingDefs,
    /// GLSL compilation options
    pub glsl_opts: GlslOpts,
    pub param_defs: MapSlot<String, ShaderParamDef>,
    /// Shader-consumed slots exposed to the resolver and GLSL uniform block.
    #[slot(name = "consumed")]
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
}

impl Default for ShaderDef {
    fn default() -> Self {
        Self {
            source: EnumSlot::new(ShaderSource::path("main.glsl")),
            render_order: RenderOrderSlot::default(),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            param_defs: MapSlot::default(),
            consumed_slots: MapSlot::default(),
        }
    }
}

impl ShaderDef {
    pub const KIND: &'static str = "shader";

    pub fn shader_source(&self) -> &ShaderSource {
        self.source.value()
    }

    pub fn render_order(&self) -> i32 {
        self.render_order.value().0
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Shader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::shader::{AddSubMode, DivMode, MulMode};
    use crate::{NodeDef, NodeKind, RenderOrder, ShaderDefView, SlotPath, SlotShapeRegistry};
    use alloc::string::ToString;

    #[test]
    fn test_shader_def_kind() {
        let def = ShaderDef {
            source: EnumSlot::new(ShaderSource::path("main.glsl")),
            render_order: RenderOrderSlot::new(RenderOrder(0)),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            param_defs: MapSlot::default(),
            consumed_slots: MapSlot::default(),
        };
        assert_eq!(def.kind(), NodeKind::Shader);
    }

    #[test]
    fn test_shader_def_default() {
        let def = ShaderDef::default();
        assert_eq!(
            def.shader_source().path_value().unwrap().as_str(),
            "main.glsl"
        );
        assert_eq!(def.render_order(), 0);
        assert_eq!(*def.glsl_opts.add_sub.value(), AddSubMode::Wrapping);
        assert_eq!(*def.glsl_opts.mul.value(), MulMode::Wrapping);
        assert_eq!(*def.glsl_opts.div.value(), DivMode::Reciprocal);
    }

    #[test]
    fn generated_shader_def_view_compiles() {
        let registry = SlotShapeRegistry::default();

        let view = ShaderDefView::compile(&registry).expect("shader def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(view.source().path(), &SlotPath::parse("source").unwrap());
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
    fn shader_def_parses_source_path() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "Shader"

source = { path = "main.glsl" }
"#,
        )
        .expect("shader");

        let NodeDef::Shader(def) = def else {
            panic!("expected shader");
        };
        assert_eq!(
            def.shader_source().path_value().unwrap().as_str(),
            "main.glsl"
        );
    }

    #[test]
    fn shader_def_parses_inline_glsl() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "Shader"

[source]
glsl = "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
"#,
        )
        .expect("shader");

        let NodeDef::Shader(def) = def else {
            panic!("expected shader");
        };
        assert!(def.shader_source().glsl_value().unwrap().contains("render"));
    }

    #[test]
    fn shader_def_rejects_glsl_path() {
        let err = NodeDef::from_toml_str(
            r#"
kind = "Shader"
glsl_path = "main.glsl"
"#,
        )
        .expect_err("glsl_path should be rejected");

        assert!(err.to_string().contains("glsl_path"));
    }
}
