use crate::{SlotValue, ValueSlot};
use serde::{Deserialize, Serialize};

/// 2D affine transform with translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = affine2d)]
pub struct Affine2d {
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2d {
    pub fn identity() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

pub type Affine2dSlot = ValueSlot<Affine2d>;
