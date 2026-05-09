//! Compiled slot view for [`TextureDef`](super::TextureDef).

use crate::{SlotAccessor, SlotShapeRegistry};

/// Compiled read-only accessors for [`TextureDef`](super::TextureDef).
pub struct TextureDefSlotView {
    pub(crate) registry_revision: crate::Revision,
    pub(crate) size_accessor: SlotAccessor,
    pub(crate) bindings_accessor: SlotAccessor,
}

impl TextureDefSlotView {
    pub fn registry_revision(&self) -> crate::Revision {
        self.registry_revision
    }

    pub fn is_valid_for(&self, registry: &SlotShapeRegistry) -> bool {
        self.registry_revision == registry.revision()
    }

    pub fn size(&self) -> &SlotAccessor {
        &self.size_accessor
    }

    pub fn bindings(&self) -> &SlotAccessor {
        &self.bindings_accessor
    }
}
