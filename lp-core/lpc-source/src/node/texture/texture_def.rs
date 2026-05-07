use crate::node::NodeKind;
use crate::node::node_def::NodeDef;
use lpc_model::{Dim2u, Dim2uSlot};
use serde::{Deserialize, Serialize};

/// Authored texture node definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, lpc_model::SlotRecord)]
#[slot(root)]
pub struct TextureDef {
    pub size: Dim2uSlot,
    // Format selection will be added when texture output semantics are revisited.
}

impl TextureDef {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: Dim2uSlot::new(Dim2u { width, height }),
        }
    }

    pub fn width(&self) -> u32 {
        self.size.value().width
    }

    pub fn height(&self) -> u32 {
        self.size.value().height
    }
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
        let def = TextureDef::new(100, 200);
        assert_eq!(def.kind(), NodeKind::Texture);
        assert_eq!(def.width(), 100);
        assert_eq!(def.height(), 200);
    }
}
