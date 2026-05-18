use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// 2D XY coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = xy)]
pub struct Xy(pub [f32; 2]);

impl From<[f32; 2]> for Xy {
    fn from(value: [f32; 2]) -> Self {
        Self(value)
    }
}

pub type XySlot = ValueSlot<Xy>;
