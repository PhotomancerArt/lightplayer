use crate::{
    ControlProduct, FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Revision-tracked graph control-product leaf.
pub type ControlProductSlot = ValueSlot<ControlProduct>;

impl ToLpValue for ControlProduct {
    fn to_lp_value(&self) -> LpValue {
        LpValue::ControlProduct(*self)
    }
}

impl FromLpValue for ControlProduct {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::ControlProduct(product) => Ok(product),
            other => Err(ValueRootError::new(alloc::format!(
                "expected ControlProduct, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ControlProduct {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.control_product");

    fn value_shape() -> SlotValueShape {
        control_product_shape()
    }
}

pub fn control_product_shape() -> SlotValueShape {
    SlotValueShape {
        id: ControlProduct::SHAPE_ID,
        ty: LpType::ControlProduct,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::ControlProduct,
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
            ControlProduct::from_lp_value(product.to_lp_value()).unwrap(),
            product
        );
        assert_eq!(control_product_shape().ty, LpType::ControlProduct);
    }
}
