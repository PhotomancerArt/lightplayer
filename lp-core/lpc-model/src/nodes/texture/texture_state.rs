//! Public runtime state shape for texture nodes.

use crate::{Revision, Slotted, ValueSlot};

/// Runtime metadata exposed by a texture node.
#[derive(Default, Slotted)]
#[slot(default_role = "state")]
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
    use crate::{SlotDirection, SlotRole, SlotShape, StaticSlotShape};

    /// TextureState is the record that proved the direction-implied rule was
    /// too quiet: it was safe only because the studio rewrote produced slots
    /// at DTO-build time. Since the G2 amendment it declares the `State` role
    /// outright, and the derive would refuse to compile it otherwise.
    #[test]
    fn texture_state_fields_declare_the_state_role_and_are_read_only() {
        let SlotShape::Record { fields, .. } = TextureState::slot_shape() else {
            panic!("record shape");
        };

        for name in ["width", "height", "format"] {
            let field = fields
                .iter()
                .find(|field| field.name.as_str() == name)
                .expect("texture state field");
            assert_eq!(field.semantics.direction, SlotDirection::Produced);
            assert_eq!(field.role, SlotRole::State);
            assert!(
                !field.is_writable(),
                "a State/produced field is never writable"
            );
        }
    }
}
