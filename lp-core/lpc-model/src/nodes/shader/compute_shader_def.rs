//! Authored serial compute shader node definition.
//!
//! A compute shader consumes and produces slot-shaped values. Runtime execution
//! is introduced later; this definition only records the authored data contract
//! and GLSL source location.

use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::nodes::shader::{GlslOpts, ShaderSlotDef};
use crate::{AsLpPathBuf, BindingDefs, LpPathBuf, MapSlot, SourcePathSlot};

/// Authored serial compute shader definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct ComputeShaderDef {
    /// Path to the GLSL source, relative to this artifact file.
    pub glsl_path: SourcePathSlot,
    /// Authored slot bindings for compute shader consumed and produced slots.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    /// GLSL compilation options.
    #[serde(default)]
    pub glsl_opts: GlslOpts,
    /// Slots resolved by this compute shader.
    #[serde(
        default,
        rename = "consumed",
        skip_serializing_if = "MapSlot::is_empty"
    )]
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
    /// Slots produced by this compute shader.
    #[serde(
        default,
        rename = "produced",
        skip_serializing_if = "MapSlot::is_empty"
    )]
    pub produced_slots: MapSlot<String, ShaderSlotDef>,
}

impl Default for ComputeShaderDef {
    fn default() -> Self {
        Self {
            glsl_path: SourcePathSlot::new(String::from("main.glsl")),
            bindings: BindingDefs::default(),
            glsl_opts: GlslOpts::default(),
            consumed_slots: MapSlot::default(),
            produced_slots: MapSlot::default(),
        }
    }
}

impl ComputeShaderDef {
    pub const KIND: &'static str = "shader/compute";

    pub fn glsl_path_buf(&self) -> LpPathBuf {
        self.glsl_path.value().as_path_buf()
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::ComputeShader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FluidEmitter, NodeDef, ShaderSlotKind, ShaderSlotMappingKind, SlotShapeRegistry,
        StaticSlotShape,
    };

    #[test]
    fn compute_shader_def_parses_consumed_and_produced_slots() {
        let def: ComputeShaderDef = toml::from_str(
            r#"
kind = "shader/compute"
glsl_path = "emitters.glsl"

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
kind = "shader/compute"
glsl_path = "emitters.glsl"
"#,
        )
        .expect("node def");

        assert!(matches!(def, NodeDef::ComputeShader(_)));
    }

    #[test]
    fn compute_shader_shape_can_reference_native_fluid_emitter() {
        let mut registry = SlotShapeRegistry::default();
        FluidEmitter::ensure_registered(&mut registry).expect("fluid emitter");
        ComputeShaderDef::ensure_registered(&mut registry).expect("compute shader");

        assert!(registry.get_by_name("lp::fluid::Emitter").is_some());
        assert!(registry.contains(&ComputeShaderDef::SHAPE_ID));
    }
}
