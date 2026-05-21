//! Native fluid emitter value shape.
//!
//! `FluidEmitter` is a complete slot value leaf. Collections of emitters should
//! be represented as `MapSlot<u32, FluidEmitter>` or equivalent map-shaped slot
//! data so each emitter has stable identity at the slot layer.

use crate::SlotValue;
use serde::{Deserialize, Serialize};

/// Native shape name used by authored shader slot defs.
pub const FLUID_EMITTER_SHAPE_NAME: &str = "lp::fluid::Emitter";

/// One fluid emission source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(shape_id = "lp::fluid::Emitter")]
pub struct FluidEmitter {
    pub id: u32,
    pub pos: [f32; 2],
    pub dir: [f32; 2],
    pub radius: f32,
    pub color: [f32; 3],
    pub velocity: f32,
    pub intensity: f32,
}

impl FluidEmitter {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pos: [0.5, 0.5],
            dir: [1.0, 0.0],
            radius: 0.05,
            color: [1.0, 1.0, 1.0],
            velocity: 0.0,
            intensity: 1.0,
        }
    }
}

impl Default for FluidEmitter {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromLpValue, ToLpValue};

    #[test]
    fn fluid_emitter_round_trips_through_lp_value() {
        let emitter = FluidEmitter {
            id: 7,
            pos: [0.25, 0.75],
            dir: [0.0, 1.0],
            radius: 0.1,
            color: [1.0, 0.5, 0.25],
            velocity: 0.2,
            intensity: 0.8,
        };

        assert_eq!(
            FluidEmitter::from_lp_value(&emitter.to_lp_value()).unwrap(),
            emitter
        );
    }

    #[test]
    fn fluid_emitter_has_static_shape_name() {
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(<FluidEmitter as SlotValue>::SHAPE_ID),
            Some(FLUID_EMITTER_SHAPE_NAME)
        );
    }
}
