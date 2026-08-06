//! Unbounded elapsed-seconds value shape.
//!
//! Unlike [`crate::TimeProduct`] (a lazy graph handle) or a future declared
//! `Phasor` shape (a wrapped `[0,1)` cycle position), `Seconds` is a plain,
//! explicit, unbounded elapsed-time scalar: no wrapping, no freeze semantics.
//! It has no dedicated Rust newtype — like `lp::clock::Rate`
//! (`nodes/clock/clock_transport.rs`), it is a named shape laid over a plain
//! `f32` field so a `ValueSlot<f32>` can opt into this shape and editor hint
//! without introducing a wrapper type.

use crate::{
    LpType, OrderedF32, SlotMeta, SlotShapeId, SlotValueShape, StaticLpType, StaticSlotMeta,
    StaticSlotValueShape, StaticValueEditorHint, ValueEditorHint,
};

/// Native shape name used by authored shader slot definitions.
pub const SECONDS_SHAPE_NAME: &str = "lp::time::Seconds";

/// Shape for an unbounded, non-negative elapsed-seconds `f32` field.
pub fn seconds_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name(SECONDS_SHAPE_NAME),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}

/// [`seconds_shape`], usable in a `const` static shape descriptor.
pub const fn static_seconds_shape() -> StaticSlotValueShape {
    StaticSlotValueShape {
        id: SlotShapeId::from_static_name(SECONDS_SHAPE_NAME),
        ty: StaticLpType::F32,
        meta: StaticSlotMeta::EMPTY,
        editor: StaticValueEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_shape_is_a_non_negative_number_leaf() {
        let shape = seconds_shape();

        assert_eq!(shape.ty, LpType::F32);
        assert!(matches!(
            shape.editor,
            ValueEditorHint::Number {
                min: Some(OrderedF32(min)),
                max: None,
                step: None,
            } if min == 0.0
        ));
    }

    #[test]
    fn static_and_dynamic_seconds_shapes_agree() {
        let dynamic = seconds_shape();
        let static_shape = static_seconds_shape();

        assert_eq!(dynamic.id, static_shape.id);
    }
}
