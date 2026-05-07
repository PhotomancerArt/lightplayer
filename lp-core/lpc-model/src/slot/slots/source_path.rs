use crate::{
    FieldSlot, FrameId, LpType, LpValue, SlotDataAccess, SlotEditorHint, SlotLeafId,
    SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned, current_state_version,
};
use alloc::string::String;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned path to an authored source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePathSlot {
    inner: Versioned<String>,
}

impl SourcePathSlot {
    pub fn new(value: String) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: String) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: String) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &String {
        self.inner.value()
    }
}

impl SlotValueAccess for SourcePathSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        LpValue::String(self.inner.value().clone())
    }
}

impl Serialize for SourcePathSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SourcePathSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(String::deserialize(deserializer)?))
    }
}

impl FieldSlot for SourcePathSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(source_path_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn source_path_shape() -> SlotValueShape {
    path_shape("slot.leaf.source_path")
}

pub(super) fn path_shape(name: &str) -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name(name),
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Path,
    }
}
