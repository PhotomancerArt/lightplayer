use crate::{
    FromLpValue, LpType, LpValue, ResourceRef, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

/// Revision-tracked resource reference.
pub type ResourceRefSlot = ValueSlot<ResourceRef>;

impl ToLpValue for ResourceRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Resource(*self)
    }
}

impl FromLpValue for ResourceRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Resource(value) => Ok(*value),
            other => Err(ValueRootError::new(alloc::format!(
                "expected Resource, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ResourceRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ResourceRef");

    fn value_shape() -> SlotValueShape {
        resource_ref_shape()
    }
}

pub fn resource_ref_shape() -> SlotValueShape {
    SlotValueShape {
        id: ResourceRef::SHAPE_ID,
        ty: LpType::Resource,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Resource,
    }
}

pub fn runtime_buffer_resource_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("RuntimeBufferResource"),
        ty: LpType::Resource,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::RuntimeBufferResource,
    }
}
