use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// Non-negative floating point value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = number(min = 0.0))]
pub struct PositiveF32(pub f32);

impl From<f32> for PositiveF32 {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

pub type PositiveF32Slot = ValueSlot<PositiveF32>;
