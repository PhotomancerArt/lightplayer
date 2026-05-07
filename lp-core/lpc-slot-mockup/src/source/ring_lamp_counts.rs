use lpc_model::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// One logical value containing the per-ring lamp counts for a generated path.
///
/// The list is editable and inspectable as value structure, but it is not a map
/// of independently versioned slots. Changing one count produces a new complete
/// value for the `ring_lamp_counts` slot.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RingLampCounts(pub Vec<u32>);

impl RingLampCounts {
    pub fn new(counts: impl Into<Vec<u32>>) -> Self {
        Self(counts.into())
    }
}

impl ToLpValue for RingLampCounts {
    fn to_lp_value(&self) -> LpValue {
        self.0.to_lp_value()
    }
}

impl FromLpValue for RingLampCounts {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        Vec::<u32>::from_lp_value(value).map(Self)
    }
}

impl SlotValue for RingLampCounts {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("mock.source.ring_lamp_counts");

    fn value_shape() -> SlotValueShape {
        ring_lamp_counts_shape()
    }
}

pub fn ring_lamp_counts_shape() -> SlotValueShape {
    SlotValueShape {
        id: RingLampCounts::SHAPE_ID,
        ty: LpType::List(Box::new(LpType::U32)),
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_lamp_counts_are_one_array_value() {
        let counts = RingLampCounts::new(vec![1, 8, 12]);

        assert_eq!(
            counts.to_lp_value(),
            LpValue::Array(vec![LpValue::U32(1), LpValue::U32(8), LpValue::U32(12)])
        );
        assert_eq!(
            RingLampCounts::from_lp_value(counts.to_lp_value()).unwrap(),
            counts
        );
        assert_eq!(
            RingLampCounts::value_shape().ty,
            LpType::List(Box::new(LpType::U32))
        );
    }
}
