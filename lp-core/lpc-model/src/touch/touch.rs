//! One finger on a drawable picture.
//!
//! A touch set is a `MapSlot<u32, Touch>` keyed by pointer id, on the
//! well-known `touches` channel ([`crate::bus::TOUCHES_CHANNEL`]). Studio
//! writes it as a **momentary** panel write (panel.md P14): it is renewed
//! while a finger rests and despawns when the writes stop, so a set is never
//! persisted.
//!
//! Positions are in pattern space (`docs/adr/2026-09-24-pattern-space.md`):
//! the lamps' box, centred, long side −1…1, y up. Velocity and direction are
//! not carried: the consumer derives them from each id's previous position
//! (panel.md P3), the way lp2014's `Touch` did.

use crate::SlotValue;
use serde::{Deserialize, Serialize};

/// Native shape name used by authored shader slot defs
/// (`map(key = "u32", value_ref = "lp::touch::Touch")`).
pub const TOUCH_SHAPE_NAME: &str = "lp::touch::Touch";

/// One touch: a pointer id and where it is, in pattern space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(shape_id = "lp::touch::Touch")]
pub struct Touch {
    /// The pointer id, repeated from the map key so a shader's sentinel
    /// mapping (`key: "id"`, `empty_key: 0`) can find it. Never 0: 0 marks
    /// an empty slot.
    pub id: u32,
    /// Position in pattern space.
    pub pos: [f32; 2],
}

impl Touch {
    pub const fn new(id: u32, pos: [f32; 2]) -> Self {
        Self { id, pos }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromLpValue, ToLpValue};

    #[test]
    fn touch_round_trips_through_lp_value() {
        let touch = Touch::new(3, [-0.25, 0.75]);
        assert_eq!(Touch::from_lp_value(&touch.to_lp_value()).unwrap(), touch);
    }

    #[test]
    fn touch_has_static_shape_name() {
        assert_eq!(
            crate::slot_shapes::static_slot_shape_name(<Touch as SlotValue>::SHAPE_ID),
            Some(TOUCH_SHAPE_NAME)
        );
    }
}
