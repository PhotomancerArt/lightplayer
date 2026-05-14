use crate::{
    FromLpValue, LpType, LpValue, ProductKind, ProductRef, SlotMeta, SlotShapeId, SlotValue,
    SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot, VisualProduct,
};

/// Revision-tracked graph visual-product leaf.
pub type VisualProductSlot = ValueSlot<VisualProduct>;

impl ToLpValue for VisualProduct {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Product(ProductRef::visual(*self))
    }
}

impl FromLpValue for VisualProduct {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Product(ProductRef::Visual(product)) => Ok(*product),
            other => Err(ValueRootError::new(alloc::format!(
                "expected VisualProduct, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for VisualProduct {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("VisualProduct");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Product(ProductKind::Visual),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::VisualProduct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[test]
    fn visual_product_round_trips_through_lp_value() {
        let product = VisualProduct::new(NodeId::new(3), 1);

        assert_eq!(
            VisualProduct::from_lp_value(&product.to_lp_value()).unwrap(),
            product
        );
        assert_eq!(
            VisualProduct::value_shape().ty,
            LpType::Product(ProductKind::Visual)
        );
    }
}
