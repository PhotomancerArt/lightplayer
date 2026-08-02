//! Public runtime state shape for texture nodes.

use crate::{Revision, Slotted, ValueSlot};

/// Runtime metadata exposed by a texture node.
#[derive(Default, Slotted)]
pub struct TextureState {
    #[slot(produced)]
    pub width: ValueSlot<i32>,
    #[slot(produced)]
    pub height: ValueSlot<i32>,
    #[slot(produced)]
    pub format: ValueSlot<u32>,
}

impl TextureState {
    pub fn new(width: i32, height: i32, format: u32) -> Self {
        Self {
            width: ValueSlot::new(width),
            height: ValueSlot::new(height),
            format: ValueSlot::new(format),
        }
    }

    pub fn sync(&mut self, width: i32, height: i32, format: u32) {
        self.width.set(width);
        self.height.set(height);
        self.format.set(format);
    }

    pub fn sync_with_revision(&mut self, revision: Revision, width: i32, height: i32, format: u32) {
        self.width.set_with_version(revision, width);
        self.height.set_with_version(revision, height);
        self.format.set_with_version(revision, format);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotDirection, SlotShape, StaticSlotShape};

    /// TextureState carries no explicit role on any field (unlike its sibling
    /// state records, which used to need a container-wide
    /// `read_only_transient` marking) — it is safe by construction because
    /// direction alone implies read-only/never-serialized (D1).
    #[test]
    fn texture_state_fields_are_produced_and_present_read_only_with_default_role() {
        let SlotShape::Record { fields, .. } = TextureState::slot_shape() else {
            panic!("record shape");
        };

        for name in ["width", "height", "format"] {
            let field = fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .expect("texture state field");
            assert_eq!(field.semantics.direction, SlotDirection::Produced);
            assert!(field.role.is_default(), "no explicit role is declared");
            assert!(
                !field.is_writable(),
                "a produced field is never writable, regardless of its default role"
            );
        }
    }
}
