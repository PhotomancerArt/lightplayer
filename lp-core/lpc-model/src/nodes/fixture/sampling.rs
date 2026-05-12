//! Fixture visual sampling strategy.

use serde::{Deserialize, Serialize};

/// How a fixture evaluates its input visual product before writing control samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureSamplingConfig {
    /// Sample the shader directly once per fixture lamp.
    Direct,
    /// Render the visual product to a texture, then area-sample the texture.
    TextureArea,
}

impl Default for FixtureSamplingConfig {
    fn default() -> Self {
        Self::TextureArea
    }
}
