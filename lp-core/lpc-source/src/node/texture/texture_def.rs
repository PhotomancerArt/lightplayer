use crate::node::NodeKind;
use crate::node::node_def::NodeDef;
use serde::{Deserialize, Serialize};

/// Authored texture node definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureDef {
    pub width: u32,
    pub height: u32,
    // Format selection will be added when texture output semantics are revisited.
}

impl NodeDef for TextureDef {
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
    fn test_texture_def_kind() {
        let def = TextureDef {
            width: 100,
            height: 200,
        };
        assert_eq!(def.kind(), NodeKind::Texture);
    }
}
