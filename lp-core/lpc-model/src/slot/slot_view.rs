//! Generic compiled slot view storage.
//!
//! A [`SlotView`] belongs to one slot root type and stores that root's compiled
//! [`SlotAccessor`] handles. Derive code adds root-specific constructor and
//! accessor methods on the root type, so callers can stay on the domain type
//! (`TextureDef::compile_slot_view`) without naming generated sibling types.

use crate::{Revision, SlotAccessor, SlotShapeRegistry};
use alloc::boxed::Box;
use core::marker::PhantomData;

/// Compiled accessors for one static slot root type.
pub struct SlotView<T> {
    registry_revision: Revision,
    accessors: Box<[SlotAccessor]>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> SlotView<T> {
    pub fn new(registry_revision: Revision, accessors: Box<[SlotAccessor]>) -> Self {
        Self {
            registry_revision,
            accessors,
            _marker: PhantomData,
        }
    }

    pub fn registry_revision(&self) -> Revision {
        self.registry_revision
    }

    pub fn is_valid_for(&self, registry: &SlotShapeRegistry) -> bool {
        self.registry_revision == registry.revision()
    }

    pub fn accessor(&self, index: usize) -> &SlotAccessor {
        &self.accessors[index]
    }
}
