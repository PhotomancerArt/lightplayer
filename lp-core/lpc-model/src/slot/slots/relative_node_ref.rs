use crate::{
    FieldSlot, FrameId, LpType, LpValue, RelativeNodeRef, SlotDataAccess, SlotEditorHint,
    SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape,
    ToLpValue, Versioned, current_state_version,
};
use alloc::string::ToString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned relative node reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeNodeRefSlot {
    inner: Versioned<RelativeNodeRef>,
}

impl RelativeNodeRefSlot {
    pub fn new(value: RelativeNodeRef) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: RelativeNodeRef) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: RelativeNodeRef) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &RelativeNodeRef {
        self.inner.value()
    }
}

impl SlotValueAccess for RelativeNodeRefSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for RelativeNodeRefSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RelativeNodeRefSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(RelativeNodeRef::deserialize(deserializer)?))
    }
}

impl FieldSlot for RelativeNodeRefSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(relative_node_ref_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for RelativeNodeRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.to_string())
    }
}

impl crate::FromLpValue for RelativeNodeRef {
    fn from_lp_value(value: LpValue) -> Result<Self, SlotLeafError> {
        match value {
            LpValue::String(value) => RelativeNodeRef::parse(&value)
                .map_err(|err| SlotLeafError::new(alloc::format!("{err}"))),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotLeaf for RelativeNodeRef {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.relative_node_ref");

    fn value_shape() -> SlotValueShape {
        relative_node_ref_shape()
    }
}

pub fn relative_node_ref_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.relative_node_ref"),
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::NodeRef,
    }
}
