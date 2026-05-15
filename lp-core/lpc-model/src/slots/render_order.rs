use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// Render ordering value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = number(step = 1.0))]
pub struct RenderOrder(pub i32);

impl From<i32> for RenderOrder {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

pub type RenderOrderSlot = ValueSlot<RenderOrder>;
