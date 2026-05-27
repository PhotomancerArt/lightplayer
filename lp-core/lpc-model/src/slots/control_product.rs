use crate::{
    ControlProduct, FromLpValue, LpType, LpValue, ProductKind, ProductRef, SlotMeta, SlotShapeId,
    SlotValue, SlotValueShape, StaticLpType, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Revision-tracked graph control-product leaf.
pub type ControlProductSlot = ValueSlot<ControlProduct>;

impl ToLpValue for ControlProduct {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Product(ProductRef::control(*self))
    }
}

impl FromLpValue for ControlProduct {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Product(ProductRef::Control(product)) => Ok(*product),
            other => Err(ValueRootError::new(alloc::format!(
                "expected ControlProduct, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ControlProduct {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ControlProduct");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::Product(ProductKind::Control),
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::ControlProduct,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Product(ProductKind::Control),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::ControlProduct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControlExtent, NodeId};

    #[test]
    fn control_product_round_trips_through_lp_value() {
        let product = ControlProduct::new(NodeId::new(3), 1, ControlExtent::new(1, 600));

        assert_eq!(
            ControlProduct::from_lp_value(&product.to_lp_value()).unwrap(),
            product
        );
        assert_eq!(
            ControlProduct::value_shape().ty,
            LpType::Product(ProductKind::Control)
        );
    }
}
