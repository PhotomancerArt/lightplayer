use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};
use alloc::boxed::Box;
use alloc::vec::Vec;

impl ToLpValue for Vec<u32> {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Array(self.iter().copied().map(LpValue::U32).collect())
    }
}

impl FromLpValue for Vec<u32> {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Array(values) = value else {
            return Err(ValueRootError::new("expected u32 list array"));
        };

        let mut output = Vec::with_capacity(values.len());
        for value in values {
            let LpValue::U32(value) = value else {
                return Err(ValueRootError::new("expected u32 list array of u32 values"));
            };
            output.push(*value);
        }
        Ok(output)
    }
}

impl SlotValue for Vec<u32> {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("VecU32");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::List(Box::new(LpType::U32)),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn u32_list_is_a_raw_slot_value() {
        let values = vec![1, 8, 12];

        assert_eq!(
            values.to_lp_value(),
            LpValue::Array(vec![LpValue::U32(1), LpValue::U32(8), LpValue::U32(12)])
        );
        assert_eq!(
            Vec::<u32>::from_lp_value(&values.to_lp_value()).unwrap(),
            values
        );
        assert_eq!(
            Vec::<u32>::value_shape().ty,
            LpType::List(Box::new(LpType::U32))
        );
    }
}
