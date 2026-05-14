use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::nodes::shader::{GlslOpts, ShaderParamDef};
use crate::{
    AsLpPathBuf, BindingDefs, LpPathBuf, MapSlot, RenderOrderSlot, SlotRecord, SourcePathSlot,
};

/// Authored shader node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SlotRecord)]
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
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub param_defs: MapSlot<String, ShaderParamDef>,
}

impl Default for ShaderDef {
    fn default() -> Self {
        Self {
            glsl_path: SourcePathSlot::new(String::from("main.glsl")),
            render_order: RenderOrderSlot::new(0),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            param_defs: MapSlot::default(),
        }
    }
}

impl ShaderDef {
    pub const KIND: &'static str = "shader";

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
            param_defs: MapSlot::default(),
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
}
