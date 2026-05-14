use crate::{SlotValue, SlotValueShape, ValueSlot};
use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Path to an authored source file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[slot_value(editor = path)]
pub struct SourcePath(pub String);

impl SourcePath {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path_buf(&self) -> crate::LpPathBuf {
        crate::AsLpPathBuf::as_path_buf(&self.0)
    }
}

impl From<String> for SourcePath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SourcePath {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

pub type SourcePathSlot = ValueSlot<SourcePath>;

pub fn source_path_shape() -> SlotValueShape {
    SourcePath::value_shape()
}
