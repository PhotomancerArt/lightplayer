use alloc::collections::BTreeMap;
use core::fmt;

use super::{SlotShape, SlotShapeId, SlotTree, SlotValidationError};

/// Registry of complete slot shape trees.
///
/// Runtime slot data refers to a shape by ID. The registry owns the corresponding
/// shape tree and is the authority for validating and traversing indexed records.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotRegistry {
    shapes: BTreeMap<SlotShapeId, SlotShape>,
}

impl SlotRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: SlotShapeId, shape: SlotShape) -> Result<(), SlotRegistryError> {
        if self.shapes.contains_key(&id) {
            return Err(SlotRegistryError::DuplicateShape(id));
        }
        self.shapes.insert(id, shape);
        Ok(())
    }

    pub fn get(&self, id: &SlotShapeId) -> Option<&SlotShape> {
        self.shapes.get(id)
    }

    pub fn contains(&self, id: &SlotShapeId) -> bool {
        self.shapes.contains_key(id)
    }

    pub fn validate_tree(&self, tree: &SlotTree) -> Result<(), SlotValidationError> {
        tree.validate(self)
    }
}

/// Error returned by [`SlotRegistry`] operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotRegistryError {
    DuplicateShape(SlotShapeId),
}

impl fmt::Display for SlotRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateShape(id) => write!(f, "duplicate slot shape id: {id}"),
        }
    }
}

impl core::error::Error for SlotRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelType;

    #[test]
    fn registry_registers_and_finds_shapes() {
        let mut registry = SlotRegistry::new();
        let id = SlotShapeId::parse("texture.config").unwrap();
        let shape = SlotShape::value(ModelType::Vec2);

        registry.register(id.clone(), shape.clone()).unwrap();

        assert!(registry.contains(&id));
        assert_eq!(registry.get(&id), Some(&shape));
    }

    #[test]
    fn registry_rejects_duplicate_shape_ids() {
        let mut registry = SlotRegistry::new();
        let id = SlotShapeId::parse("texture.config").unwrap();

        registry
            .register(id.clone(), SlotShape::value(ModelType::Vec2))
            .unwrap();

        let error = registry
            .register(id.clone(), SlotShape::value(ModelType::Vec3))
            .unwrap_err();
        assert_eq!(error, SlotRegistryError::DuplicateShape(id));
    }

    #[test]
    fn registry_validates_trees() {
        use crate::{FrameId, ModelValue, SlotData, SlotTree, Versioned};

        let mut registry = SlotRegistry::new();
        let id = SlotShapeId::parse("enabled").unwrap();
        registry
            .register(id.clone(), SlotShape::value(ModelType::Bool))
            .unwrap();

        let tree = SlotTree::new(
            id,
            SlotData::Value(Versioned::new(FrameId::new(1), ModelValue::Bool(true))),
        );

        registry.validate_tree(&tree).unwrap();
    }
}
