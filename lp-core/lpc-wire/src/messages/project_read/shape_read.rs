//! Shape registry read query/result.

use super::ReadLevel;
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
