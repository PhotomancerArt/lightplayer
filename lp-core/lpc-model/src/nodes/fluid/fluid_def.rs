use crate::nodes::shader::ShaderCoords;
use crate::{
    BindingDefs, Dim2u, Dim2uSlot, FluidEmitter, MapSlot, OptionSlot, PositiveF32, PositiveF32Slot,
    Ratio, RatioSlot, Slotted, TimeProductSlot, ValueSlot,
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

    /// Passes of each pressure solve per step (red-black, over-relaxed).
    /// 3–4 look like the ten plain passes the MSAFluid reference ran; 2 and
    /// fewer show striations and harder edges.
    pub solver_iterations: ValueSlot<u32>,

    /// Target simulation update rate in Hz.
    pub step_hz: PositiveF32Slot,

    /// Per-step color fade.
    pub fade_speed: RatioSlot,

    /// Fluid viscosity.
    pub viscosity: PositiveF32Slot,

    /// Timebase the simulation steps against — the scope's time product,
    /// queried for effective seconds (the solver gates on elapsed wall time
    /// and guards against a rewind, so it needs Seconds, not a delta).
    #[slot(consumed, default_bind = "bus:time")]
    pub time: TimeProductSlot,

    /// Stable-key emitter map consumed by the fluid simulation.
    #[slot(
        consumed,
        merge = "by_key",
        map(key = "u32", value_ref = "lp::fluid::Emitter")
    )]
    pub emitters: MapSlot<u32, FluidEmitter>,

    /// Optional opt-in to pattern space (`"coords": "pattern"`), the same
    /// key and values as the shader def's (ADR
    /// `docs/adr/2026-09-24-pattern-space.md`). Absent (the state of every
    /// fluid authored before the key existed) is [`ShaderCoords::Pixels`]:
    /// the grid stretched over the render request and emitters normalized
    /// over the grid, y down — byte-identical to before. Pattern: the grid
    /// spans the lamps' box and emitters are read in pattern units, y up.
    pub coords: OptionSlot<ValueSlot<ShaderCoords>>,
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
            coords: OptionSlot::none(),
        }
    }
}

impl FluidDef {
    pub const KIND: &'static str = "fluid";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Fluid
    }

    /// The authored coordinate frame; absent reads as
    /// [`ShaderCoords::Pixels`].
    #[must_use]
    pub fn coords(&self) -> ShaderCoords {
        self.coords
            .data
            .as_ref()
            .map_or(ShaderCoords::Pixels, |slot| *slot.value())
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

fn default_time() -> TimeProductSlot {
    TimeProductSlot::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NodeDef, SlotDirection, SlotMerge, SlotShape, SlotShapeRegistry, StaticSlotShape,
        StaticSlotShapeDescriptor,
    };

    #[test]
    fn fluid_def_parses_inline_emitters() {
        let def = NodeDef::from_json_str(
            r#"{
  "kind": "Fluid",
  "emitters": {
    "1": {
      "id": 1,
      "pos": [0.5, 0.5],
      "dir": [1.0, 0.0],
      "radius": 0.05,
      "color": [1.0, 0.25, 0.0],
      "velocity": 0.4,
      "intensity": 1.0
    }
  }
}"#,
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

        let static_shape =
            crate::slot_shapes::static_slot_shape(FluidDef::SHAPE_ID).expect("static shape");
        let StaticSlotShapeDescriptor::Record { fields, .. } = *static_shape else {
            panic!("static fluid shape");
        };
        let emitters = fields
            .iter()
            .find(|field| field.name == "emitters")
            .expect("static emitters field");
        let StaticSlotShapeDescriptor::Map { value, .. } = *emitters.shape else {
            panic!("static emitters map");
        };
        assert_eq!(
            value,
            &StaticSlotShapeDescriptor::Ref {
                id: FluidEmitter::SHAPE_ID
            }
        );
    }

    /// The pattern-space opt-in parses from `"coords": "pattern"`, and a def
    /// without the key reads as Pixels (the old frame).
    #[test]
    fn fluid_def_parses_the_pattern_space_opt_in() {
        let NodeDef::Fluid(def) =
            NodeDef::from_json_str(r#"{ "kind": "Fluid", "coords": "pattern" }"#).expect("fluid")
        else {
            panic!("fluid def");
        };
        assert_eq!(def.coords(), ShaderCoords::Pattern);

        let NodeDef::Fluid(def) = NodeDef::from_json_str(r#"{ "kind": "Fluid" }"#).expect("fluid")
        else {
            panic!("fluid def");
        };
        assert!(def.coords.is_none());
        assert_eq!(def.coords(), ShaderCoords::Pixels);
    }

    /// Additive and optional: a fluid authored before the key existed writes
    /// back without it (old files round-trip unchanged — no format bump),
    /// and an authored one writes back as the string it was read from.
    #[test]
    fn coords_key_is_written_only_when_authored() {
        let registry = SlotShapeRegistry::default();
        let without = NodeDef::Fluid(FluidDef::default())
            .write_json(&registry)
            .expect("write");
        assert!(!without.contains("coords"), "{without}");
        let old = include_str!("../../../../../projects/test/fluid/fluid.json");
        let written = NodeDef::from_json_str(old)
            .expect("the shipped test fluid parses")
            .write_json(&registry)
            .expect("write");
        assert!(!written.contains("coords"), "{written}");

        let with = NodeDef::Fluid(FluidDef {
            coords: OptionSlot::some(ValueSlot::new(ShaderCoords::Pattern)),
            ..FluidDef::default()
        })
        .write_json(&registry)
        .expect("write");
        assert!(with.contains(r#""coords": "pattern""#), "{with}");
        let NodeDef::Fluid(back) = NodeDef::from_json_str(&with).expect("read back") else {
            panic!("fluid def");
        };
        assert_eq!(back.coords(), ShaderCoords::Pattern);
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
