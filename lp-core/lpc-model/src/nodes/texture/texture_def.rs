use crate::{BindingDefs, Dim2u, Dim2uSlot};
use serde::{Deserialize, Serialize};

/// Authored texture node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct TextureDef {
    pub size: Dim2uSlot,
    /// Authored slot bindings for texture materialization.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,
    // Format selection will be added when texture output semantics are revisited.
}

impl TextureDef {
    pub const KIND: &'static str = "texture";

    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: Dim2uSlot::new(Dim2u { width, height }),
            bindings: BindingDefs::default(),
        }
    }

    pub fn width(&self) -> u32 {
        self.size.value().width
    }

    pub fn height(&self) -> u32 {
        self.size.value().height
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;

    #[test]
    fn test_texture_def_kind() {
        let def = TextureDef::new(100, 200);
        assert_eq!(def.kind(), NodeKind::Texture);
        assert_eq!(def.width(), 100);
        assert_eq!(def.height(), 200);
    }
}
