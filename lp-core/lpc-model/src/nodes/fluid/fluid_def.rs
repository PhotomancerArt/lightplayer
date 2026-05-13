use serde::{Deserialize, Serialize};

use crate::{
    BindingDefs, Dim2u, Dim2uSlot, FluidEmitter, MapSlot, PositiveF32Slot, RatioSlot, ValueSlot,
};

/// Authored fluid simulation node definition.
///
/// `emitters` is real authored/default slot data and a consumed dataflow slot.
/// Most projects bind it from compute/input nodes, but inline emitter maps are
/// useful for simple scenes and tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct FluidDef {
    /// Authored slot bindings for fluid inputs and visual output.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,

    /// Solver grid size.
    #[serde(default = "default_size")]
    pub size: Dim2uSlot,

    /// Jacobi iterations per solver step.
    #[serde(default = "default_solver_iterations")]
    pub solver_iterations: ValueSlot<u32>,

    /// Target simulation update rate in Hz.
    #[serde(default = "default_step_hz")]
    pub step_hz: PositiveF32Slot,

    /// Per-step color fade.
    #[serde(default = "default_fade_speed")]
    pub fade_speed: RatioSlot,

    /// Fluid viscosity.
    #[serde(default = "default_viscosity")]
    pub viscosity: PositiveF32Slot,

    /// Stable-key emitter map consumed by the fluid simulation.
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::fluid::Emitter")
    )]
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub emitters: MapSlot<u32, FluidEmitter>,
}

impl Default for FluidDef {
    fn default() -> Self {
        Self {
            bindings: BindingDefs::default(),
            size: default_size(),
            solver_iterations: default_solver_iterations(),
            step_hz: default_step_hz(),
            fade_speed: default_fade_speed(),
            viscosity: default_viscosity(),
            emitters: MapSlot::default(),
        }
    }
}

impl FluidDef {
    pub const KIND: &'static str = "fluid";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Fluid
    }
}

fn default_size() -> Dim2uSlot {
    Dim2uSlot::new(Dim2u {
        width: 20,
        height: 20,
    })
}

fn default_solver_iterations() -> ValueSlot<u32> {
    ValueSlot::new(3)
}

fn default_step_hz() -> PositiveF32Slot {
    PositiveF32Slot::new(25.0)
}

fn default_fade_speed() -> RatioSlot {
    RatioSlot::new(0.1)
}

fn default_viscosity() -> PositiveF32Slot {
    PositiveF32Slot::new(0.00003)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDef, SlotDirection, SlotMerge, SlotShape, SlotShapeRegistry, StaticSlotShape};

    #[test]
    fn fluid_def_parses_inline_emitters() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "fluid"

[emitters.1]
id = 1
pos = [0.5, 0.5]
dir = [1.0, 0.0]
radius = 0.05
color = [1.0, 0.25, 0.0]
velocity = 0.4
intensity = 1.0
"#,
        )
        .expect("fluid");

        let NodeDef::Fluid(def) = def else {
            panic!("fluid def");
        };
        assert_eq!(def.emitters.entries.len(), 1);
        assert_eq!(def.emitters.entries.get(&1).expect("emitter").id, 1);
    }

    #[test]
    fn fluid_emitters_shape_is_consumed_and_merged_by_key() {
        let mut registry = SlotShapeRegistry::default();
        crate::slot_shapes::register_all_static_slot_shapes(&mut registry).expect("static shapes");
        assert!(registry.get_by_name("lp::fluid::Emitter").is_some());

        let SlotShape::Record { fields, .. } = FluidDef::slot_shape() else {
            panic!("record shape");
        };
        let emitters = fields
            .iter()
            .find(|field| field.name.as_str() == "emitters")
            .expect("emitters field");

        assert_eq!(emitters.semantics.direction, SlotDirection::Consumed);
        assert_eq!(emitters.semantics.merge, SlotMerge::ByKey);
    }
}
