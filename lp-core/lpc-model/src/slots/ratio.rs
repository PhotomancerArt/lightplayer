use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// Floating point ratio in the inclusive `0.0..=1.0` domain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct Ratio(pub f32);

impl From<f32> for Ratio {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

pub type RatioSlot = ValueSlot<Ratio>;
