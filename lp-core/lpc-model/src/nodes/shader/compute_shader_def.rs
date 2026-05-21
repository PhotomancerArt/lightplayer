//! Authored serial compute shader node definition.
//!
//! A compute shader consumes and produces slot-shaped values. Runtime execution
//! is introduced later; this definition only records the authored data contract
//! and GLSL source location.

use alloc::string::String;

use crate::nodes::shader::{GlslOpts, ShaderSlotDef, ShaderSource};
use crate::{BindingDefs, EnumSlot, MapSlot, Slotted};

/// Authored serial compute shader definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ComputeShaderDef {
    /// Authored shader source.
    pub source: EnumSlot<ShaderSource>,
    /// Authored slot bindings for compute shader consumed and produced slots.
    pub bindings: BindingDefs,
    /// GLSL compilation options.
    pub glsl_opts: GlslOpts,
    /// Slots resolved by this compute shader.
    #[slot(name = "consumed")]
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
    /// Slots produced by this compute shader.
    #[slot(name = "produced")]
    pub produced_slots: MapSlot<String, ShaderSlotDef>,
}

impl Default for ComputeShaderDef {
    fn default() -> Self {
        Self {
            source: EnumSlot::new(ShaderSource::path("main.glsl")),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            consumed_slots: MapSlot::default(),
            produced_slots: MapSlot::default(),
        }
    }
}

impl ComputeShaderDef {
    pub const KIND: &'static str = "shader/compute";

    pub fn shader_source(&self) -> &ShaderSource {
        self.source.value()
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::ComputeShader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FluidEmitter, NodeDef, ShaderSlotKind, ShaderSlotMappingKind, SlotShapeLookup,
        SlotShapeRegistry, StaticSlotShape,
    };
    use alloc::string::ToString;

    #[test]
    fn compute_shader_def_parses_consumed_and_produced_slots() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "ComputeShader"
source = { path = "emitters.glsl" }

[consumed.time]
kind = "value"
value = "f32"

[produced.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
"#,
        )
        .expect("compute shader");
        let NodeDef::ComputeShader(def) = def else {
            panic!("compute shader def");
        };

        assert_eq!(def.kind(), crate::NodeKind::ComputeShader);
        assert_eq!(def.consumed_slots.entries.len(), 1);
        assert_eq!(def.produced_slots.entries.len(), 1);

        let emitters = def.produced_slots.entries.get("emitters").unwrap();
        assert_eq!(*emitters.kind.value(), ShaderSlotKind::Map);
        assert_eq!(emitters.value.value().as_str(), "lp::fluid::Emitter");
        let mapping = emitters.mapping.data.as_ref().expect("mapping");
        assert_eq!(*mapping.kind.value(), ShaderSlotMappingKind::Sentinel);
    }

    #[test]
    fn node_def_parses_compute_shader_variant() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "ComputeShader"

source = { path = "emitters.glsl" }
"#,
        )
        .expect("node def");

        assert!(matches!(def, NodeDef::ComputeShader(_)));
    }

    #[test]
    fn compute_shader_def_rejects_glsl_path() {
        let err = NodeDef::from_toml_str(
            r#"
kind = "ComputeShader"
glsl_path = "emitters.glsl"
"#,
        )
        .expect_err("glsl_path should be rejected");

        assert!(err.to_string().contains("glsl_path"));
    }

    #[test]
    fn compute_shader_shape_can_reference_native_fluid_emitter() {
        let registry = SlotShapeRegistry::default();

        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(FluidEmitter::SHAPE_ID),
            Some("lp::fluid::Emitter")
        );
        assert!(registry.get_shape(ComputeShaderDef::SHAPE_ID).is_some());
    }
}
