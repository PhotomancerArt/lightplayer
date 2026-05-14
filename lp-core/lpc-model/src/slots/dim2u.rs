use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// Width/height dimensions in unsigned integer pixels or cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = dimensions)]
pub struct Dim2u {
    pub width: u32,
    pub height: u32,
}

pub type Dim2uSlot = ValueSlot<Dim2u>;
