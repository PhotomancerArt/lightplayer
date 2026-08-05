use crate::{
    FromLpValue, LpType, LpValue, ProductKind, ProductRef, SlotMeta, SlotShapeId, SlotValue,
    SlotValueShape, StaticLpType, StaticSlotMeta, StaticSlotValueShape, StaticValueEditorHint,
    TimeProduct, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Revision-tracked graph time-product leaf.
pub type TimeProductSlot = ValueSlot<TimeProduct>;

impl ToLpValue for TimeProduct {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Product(ProductRef::time(*self))
    }
}

impl FromLpValue for TimeProduct {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Product(ProductRef::Time(product)) => Ok(*product),
            other => Err(ValueRootError::new(alloc::format!(
                "expected TimeProduct, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for TimeProduct {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("TimeProduct");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::Product(ProductKind::Time),
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Plain,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Product(ProductKind::Time),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[test]
    fn time_product_round_trips_through_lp_value() {
        let product = TimeProduct::new(NodeId::new(3), 1);

        assert_eq!(
            TimeProduct::from_lp_value(&product.to_lp_value()).unwrap(),
            product
        );
        assert_eq!(
            TimeProduct::value_shape().ty,
            LpType::Product(ProductKind::Time)
        );
    }
}
