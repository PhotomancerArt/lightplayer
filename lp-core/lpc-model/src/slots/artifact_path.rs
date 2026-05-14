use crate::{SlotValue, SlotValueShape, ValueSlot};
use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Path to an authored artifact file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = path)]
pub struct ArtifactPath(pub String);

impl ArtifactPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ArtifactPath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ArtifactPath {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

pub type ArtifactPathSlot = ValueSlot<ArtifactPath>;

pub fn artifact_path_shape() -> SlotValueShape {
    ArtifactPath::value_shape()
}
