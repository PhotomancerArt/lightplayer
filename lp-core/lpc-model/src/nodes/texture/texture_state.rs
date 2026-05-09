//! Public runtime state shape for texture nodes.

use crate::{Revision, ValueSlot};

/// Runtime metadata exposed by a texture node.
#[derive(lpc_slot_macros::SlotRecord)]
#[slot(root)]
pub struct TextureState {
    pub width: ValueSlot<i32>,
    pub height: ValueSlot<i32>,
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
