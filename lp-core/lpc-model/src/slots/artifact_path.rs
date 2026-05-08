use crate::{
    FieldSlot, Revision, LpValue, SlotDataAccess, SlotShape, SlotValueAccess, WithRevision,
    current_revision,
};
use alloc::string::String;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::source_path::path_shape;

/// Versioned path to an authored artifact file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPathSlot {
    inner: WithRevision<String>,
}

impl ArtifactPathSlot {
    pub fn new(value: String) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(frame: Revision, value: String) -> Self {
        Self {
            inner: WithRevision::new(frame, value),
        }
    }

    pub fn set(&mut self, value: String) {
        self.inner.set(current_revision(), value);
    }

    pub fn changed_frame(&self) -> Revision {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &String {
        self.inner.value()
    }
}

impl SlotValueAccess for ArtifactPathSlot {
    fn changed_frame(&self) -> Revision {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        LpValue::String(self.inner.value().clone())
    }
}

impl Serialize for ArtifactPathSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ArtifactPathSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(String::deserialize(deserializer)?))
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ArtifactPathSlot {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <String as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

impl FieldSlot for ArtifactPathSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(artifact_path_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn artifact_path_shape() -> crate::SlotValueShape {
    path_shape("slot.leaf.artifact_path")
}
