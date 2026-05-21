//! Common lookup surface for static catalog plus dynamic shape overlays.

use crate::{Revision, SlotFactoryError, SlotMutAccess, SlotShapeId, SlotShapeView};
use alloc::boxed::Box;

/// Read and factory access for slot shapes.
///
/// Implementations may resolve shapes from a dynamic registry, a generated
/// static catalog, or a combination of both.
pub trait SlotShapeLookup {
    fn revision(&self) -> Revision;

    fn get_shape(&self, id: SlotShapeId) -> Option<SlotShapeView<'_>>;

    fn contains_shape(&self, id: SlotShapeId) -> bool {
        self.get_shape(id).is_some()
    }

    fn create_default(&self, id: SlotShapeId) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LpType, SlotShape, SlotShapeRegistry};

    #[test]
    fn registry_lookup_exposes_dynamic_shape_view() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::new(7);
        registry
            .register_dynamic_shape(id, SlotShape::value(LpType::Bool))
            .unwrap();

        let view = SlotShapeLookup::get_shape(&registry, id).expect("shape");

        assert!(view.value_shape().is_some());
        assert!(SlotShapeLookup::contains_shape(&registry, id));
    }
}
