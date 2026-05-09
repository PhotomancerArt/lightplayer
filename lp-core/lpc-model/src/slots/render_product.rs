use crate::{
    FromLpValue, LpType, LpValue, RenderProduct, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Revision-tracked graph render-product leaf.
pub type RenderProductSlot = ValueSlot<RenderProduct>;

impl ToLpValue for RenderProduct {
    fn to_lp_value(&self) -> LpValue {
        LpValue::RenderProduct(*self)
    }
}

impl FromLpValue for RenderProduct {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::RenderProduct(product) => Ok(product),
            other => Err(ValueRootError::new(alloc::format!(
                "expected RenderProduct, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for RenderProduct {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.render_product");

    fn value_shape() -> SlotValueShape {
        render_product_shape()
    }
}

pub fn render_product_shape() -> SlotValueShape {
    SlotValueShape {
        id: RenderProduct::SHAPE_ID,
        ty: LpType::RenderProduct,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::RenderProduct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[test]
    fn render_product_round_trips_through_lp_value() {
        let product = RenderProduct::new(NodeId::new(3), 1);

        assert_eq!(
            RenderProduct::from_lp_value(product.to_lp_value()).unwrap(),
            product
        );
        assert_eq!(render_product_shape().ty, LpType::RenderProduct);
    }
}
