//! Shape registry read query/result.

use super::ReadLevel;
use lpc_model::{SlotShapeId, SlotShapeRegistrySnapshot};

/// Request for slot shape registry data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ShapeReadQuery {
    pub level: ReadLevel,
    /// Exclusive cursor from a previous [`ShapeReadResult::next`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<SlotShapeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl Default for ShapeReadQuery {
    fn default() -> Self {
        Self {
            level: ReadLevel::Summary,
            after: None,
            limit: None,
        }
    }
}

/// Slot shape registry data returned by a read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ShapeReadResult {
    pub level: ReadLevel,
    pub registry: Option<SlotShapeRegistrySnapshot>,
    #[serde(default = "default_complete_shape_read")]
    pub complete: bool,
    /// Cursor to pass as the next request's [`ShapeReadQuery::after`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<SlotShapeId>,
}

fn default_complete_shape_read() -> bool {
    true
}
