use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, StaticLpType,
    StaticSlotMeta, StaticSlotValueShape, StaticValueEditorHint, ToLpValue, ValueEditorHint,
    ValueRootError, ValueSlot,
};
use serde::{Deserialize, Serialize};

const AFFINE_EPSILON: f32 = 1.0e-5;

/// 2D affine transform with translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

impl Default for Affine2d {
    fn default() -> Self {
        Self::identity()
    }
}

impl ToLpValue for Affine2d {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Mat3x3([
            [self.m00, self.m01, self.tx],
            [self.m10, self.m11, self.ty],
            [0.0, 0.0, 1.0],
        ])
    }
}

impl FromLpValue for Affine2d {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Mat3x3(matrix) = value else {
            return Err(ValueRootError::new(alloc::format!(
                "expected Mat3x3, got {value:?}"
            )));
        };

        validate_affine_row(matrix[2])?;
        Ok(Self {
            m00: matrix[0][0],
            m01: matrix[0][1],
            m10: matrix[1][0],
            m11: matrix[1][1],
            tx: matrix[0][2],
            ty: matrix[1][2],
        })
    }
}

impl SlotValue for Affine2d {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("Affine2d");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::Mat3x3,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Affine2d,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Mat3x3,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Affine2d,
        }
    }
}

pub type Affine2dSlot = ValueSlot<Affine2d>;

fn validate_affine_row(row: [f32; 3]) -> Result<(), ValueRootError> {
    if nearly_eq(row[0], 0.0) && nearly_eq(row[1], 0.0) && nearly_eq(row[2], 1.0) {
        return Ok(());
    }

    Err(ValueRootError::new(alloc::format!(
        "expected affine Mat3x3 bottom row close to [0, 0, 1], got {row:?}"
    )))
}

fn nearly_eq(a: f32, b: f32) -> bool {
    (a - b).abs() <= AFFINE_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine2d_uses_mat3x3_slot_value_shape() {
        assert_eq!(Affine2d::value_shape().ty, LpType::Mat3x3);
        assert_eq!(Affine2d::value_shape().editor, ValueEditorHint::Affine2d);
    }

    #[test]
    fn affine2d_round_trips_as_mat3x3_lp_value() {
        let affine = Affine2d {
            m00: 1.0,
            m01: 0.25,
            m10: -0.5,
            m11: 2.0,
            tx: 12.0,
            ty: -8.0,
        };

        assert_eq!(
            affine.to_lp_value(),
            LpValue::Mat3x3([[1.0, 0.25, 12.0], [-0.5, 2.0, -8.0], [0.0, 0.0, 1.0]])
        );
        assert_eq!(
            Affine2d::from_lp_value(&affine.to_lp_value()).unwrap(),
            affine
        );
    }

    #[test]
    fn affine2d_accepts_fuzzy_affine_bottom_row() {
        let value = LpValue::Mat3x3([
            [1.0, 0.0, 2.0],
            [0.0, 1.0, 3.0],
            [0.000_001, -0.000_001, 0.999_999],
        ]);

        assert_eq!(
            Affine2d::from_lp_value(&value).unwrap(),
            Affine2d {
                m00: 1.0,
                m01: 0.0,
                m10: 0.0,
                m11: 1.0,
                tx: 2.0,
                ty: 3.0,
            }
        );
    }

    #[test]
    fn affine2d_rejects_perspective_matrix() {
        let value = LpValue::Mat3x3([[1.0, 0.0, 2.0], [0.0, 1.0, 3.0], [0.0, 0.2, 1.0]]);

        let error = Affine2d::from_lp_value(&value).unwrap_err();

        assert!(
            error
                .message
                .contains("expected affine Mat3x3 bottom row close to [0, 0, 1]")
        );
    }
}
