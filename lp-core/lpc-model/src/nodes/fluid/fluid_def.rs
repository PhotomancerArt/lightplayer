use crate::{
    BindingDefs, Dim2u, Dim2uSlot, FluidEmitter, MapSlot, PositiveF32, PositiveF32Slot, Ratio,
    RatioSlot, Slotted, ValueSlot,
};

/// Authored fluid simulation node definition.
///
/// `emitters` is real authored/default slot data and a consumed dataflow slot.
/// Most projects bind it from compute/input nodes, but inline emitter maps are
/// useful for simple scenes and tests.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct FluidDef {
    /// Authored slot bindings for fluid inputs and visual output.
    pub bindings: BindingDefs,

    /// Solver grid size.
    pub size: Dim2uSlot,

    /// Jacobi iterations per solver step.
    pub solver_iterations: ValueSlot<u32>,

    /// Target simulation update rate in Hz.
    pub step_hz: PositiveF32Slot,

    /// Per-step color fade.
    pub fade_speed: RatioSlot,

    /// Fluid viscosity.
    pub viscosity: PositiveF32Slot,

    /// Simulation time in seconds.
    #[slot(consumed)]
    pub time: ValueSlot<f32>,

    /// Stable-key emitter map consumed by the fluid simulation.
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::fluid::Emitter")
    )]
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
            time: default_time(),
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
    PositiveF32Slot::new(PositiveF32(25.0))
}

fn default_fade_speed() -> RatioSlot {
    RatioSlot::new(Ratio(0.1))
}

fn default_viscosity() -> PositiveF32Slot {
    PositiveF32Slot::new(PositiveF32(0.00003))
}

fn default_time() -> ValueSlot<f32> {
    ValueSlot::new(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDef, SlotDirection, SlotMerge, SlotShape, StaticSlotShape};

    #[test]
    fn fluid_def_parses_inline_emitters() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "Fluid"

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
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(FluidEmitter::SHAPE_ID),
            Some("lp::fluid::Emitter")
        );

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

    #[test]
    fn fluid_time_shape_is_consumed_latest() {
        let SlotShape::Record { fields, .. } = FluidDef::slot_shape() else {
            panic!("record shape");
        };
        let time = fields
            .iter()
            .find(|field| field.name.as_str() == "time")
            .expect("time field");

        assert_eq!(time.semantics.direction, SlotDirection::Consumed);
        assert_eq!(time.semantics.merge, SlotMerge::Latest);
    }
}
