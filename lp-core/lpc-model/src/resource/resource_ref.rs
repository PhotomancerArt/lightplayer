use crate::resource::resource_domain::ResourceDomain;
use crate::resources::buffer::RuntimeBufferId;
use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Stable resource reference: domain plus raw id (no generation).
///
/// Ids are not reused within a loaded project runtime; removed ids stay invalid.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ResourceRef {
    pub domain: ResourceDomain,
    pub id: u32,
}

impl ResourceRef {
    #[must_use]
    pub const fn runtime_buffer(buffer_id: RuntimeBufferId) -> Self {
        Self {
            domain: ResourceDomain::RuntimeBuffer,
            id: buffer_id.as_u32(),
        }
    }
}

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
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Resource,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Resource,
        }
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

#[cfg(test)]
mod tests {
    use crate::resource::resource_domain::ResourceDomain;
    use crate::resource::resource_ref::ResourceRef;
    use crate::resources::buffer::RuntimeBufferId;
    use crate::{FromLpValue, SlotValue, ToLpValue, ValueEditorHint};

    #[test]
    fn resource_ref_covers_runtime_buffer() {
        let buf = RuntimeBufferId::new(7);
        let rbuf = ResourceRef::runtime_buffer(buf);
        assert_eq!(rbuf.domain, ResourceDomain::RuntimeBuffer);
        assert_eq!(rbuf.id, 7);
    }

    #[test]
    fn resource_ref_is_slot_value() {
        let resource = ResourceRef::runtime_buffer(RuntimeBufferId::new(7));

        assert_eq!(resource.to_lp_value(), crate::LpValue::Resource(resource));
        assert_eq!(
            ResourceRef::from_lp_value(&resource.to_lp_value()),
            Ok(resource)
        );
        assert_eq!(ResourceRef::value_shape().id, ResourceRef::SHAPE_ID);
        assert_eq!(ResourceRef::value_shape().editor, ValueEditorHint::Resource);
    }
}
