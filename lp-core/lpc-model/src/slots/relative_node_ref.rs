use crate::{
    LpType, LpValue, RelativeNodeRef, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError, ValueSlot,
};
use alloc::string::ToString;

/// Revision-tracked relative node reference.
pub type RelativeNodeRefSlot = ValueSlot<RelativeNodeRef>;

impl ToLpValue for RelativeNodeRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.to_string())
    }
}

impl crate::FromLpValue for RelativeNodeRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => RelativeNodeRef::parse(&value)
                .map_err(|err| ValueRootError::new(alloc::format!("{err}"))),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for RelativeNodeRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("RelativeNodeRef");

    fn value_shape() -> SlotValueShape {
        relative_node_ref_shape()
    }
}

pub fn relative_node_ref_shape() -> SlotValueShape {
    SlotValueShape {
        id: RelativeNodeRef::SHAPE_ID,
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::NodeRef,
    }
}
