//! Shape registry read query/result.

use super::ReadLevel;
use lpc_model::SlotShapeRegistrySnapshot;

/// Request for slot shape registry data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ShapeReadQuery {
    pub level: ReadLevel,
}

impl Default for ShapeReadQuery {
    fn default() -> Self {
        Self {
            level: ReadLevel::Summary,
        }
    }
}

/// Slot shape registry data returned by a read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ShapeReadResult {
    pub level: ReadLevel,
    pub registry: Option<SlotShapeRegistrySnapshot>,
}
