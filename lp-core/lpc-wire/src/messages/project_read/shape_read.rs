//! Shape registry read query/result.

use super::ReadLevel;
use alloc::vec::Vec;
use lpc_model::{SlotShapeId, SlotShapeRegistrySnapshot};

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
///
/// `membership`, when present, is the full current set of shape ids. The server sends it only when
/// the registry's `ids_revision` moved past the request `since`, so a client can prune shapes
/// removed since it last synced (revision-gated reads, G3/G7). `None` means membership is unchanged
/// and absence of an id carries no meaning. Mirrors [`super::ResourceReadResult::membership`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ShapeReadResult {
    pub level: ReadLevel,
    pub registry: Option<SlotShapeRegistrySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub membership: Option<Vec<SlotShapeId>>,
}
