use crate::nodes::{NodeConfig, NodeKind};
use serde::{Deserialize, Serialize};

/// Texture node configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureConfig {
    pub width: u32,
    pub height: u32,
    // format: todo!() - will be added later
}

impl NodeConfig for TextureConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Texture
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_config_kind() {
        let config = TextureConfig {
            width: 100,
            height: 200,
        };
        assert_eq!(config.kind(), NodeKind::Texture);
    }
}
