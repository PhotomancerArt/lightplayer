use crate::{
    FrameId, SlotShape, SlotShapeId, SlotTree, SlotValidationError, current_state_version,
};
use alloc::collections::BTreeMap;

/// Shape root plus the frame where that root last changed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct VersionedSlotShape {
    pub node: SlotShape,
    pub changed_frame: FrameId,
}

/// Registry of id-addressed slot shape roots.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistry {
    pub ids_changed_frame: FrameId,
    shapes: BTreeMap<SlotShapeId, VersionedSlotShape>,
}

impl SlotShapeRegistry {
    /// Register a new shape root.
    ///
    /// This is intentionally strict: registering an id twice is an error even
    /// when the shape is identical. Use [`Self::ensure_tree`] for static shape
    /// bootstrap code that may be called more than once.
    pub fn register_tree(
        &mut self,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_tree_with_version(current_state_version(), root, shape)
    }

    pub fn register_tree_with_version(
        &mut self,
        frame: FrameId,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&root) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(root));
        }
        self.shapes.insert(
            root,
            VersionedSlotShape {
                node: shape,
                changed_frame: frame,
            },
        );
        self.ids_changed_frame = frame;
        Ok(())
    }

    /// Ensure that a static shape root is present.
    ///
    /// Returns `Ok(true)` when the shape was inserted and `Ok(false)` when the
    /// same shape was already registered. If the id is already registered with
    /// a different shape, this returns a conflict error rather than replacing
    /// the existing shape.
    pub fn ensure_tree(
        &mut self,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_tree_with_version(current_state_version(), root, shape)
    }

    pub fn ensure_tree_with_version(
        &mut self,
        frame: FrameId,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        if let Some(existing) = self.shapes.get(&root) {
            return if existing.node == shape {
                Ok(false)
            } else {
                Err(SlotShapeRegistryError::ShapeIdConflict(root))
            };
        }

        self.shapes.insert(
            root,
            VersionedSlotShape {
                node: shape,
                changed_frame: frame,
            },
        );
        self.ids_changed_frame = frame;
        Ok(true)
    }

    /// Replace a dynamic shape root.
    ///
    /// Runtime-owned shapes whose structure varies by artifact or instance use
    /// this path when their shape changes.
    pub fn replace_tree(&mut self, root: SlotShapeId, shape: SlotShape) {
        self.replace_tree_with_version(current_state_version(), root, shape);
    }

    pub fn replace_tree_with_version(
        &mut self,
        frame: FrameId,
        root: SlotShapeId,
        shape: SlotShape,
    ) {
        self.shapes.insert(
            root,
            VersionedSlotShape {
                node: shape,
                changed_frame: frame,
            },
        );
        self.ids_changed_frame = frame;
    }

    pub fn unregister_tree(&mut self, root: &SlotShapeId) {
        self.unregister_tree_with_version(current_state_version(), root);
    }

    pub fn unregister_tree_with_version(&mut self, frame: FrameId, root: &SlotShapeId) {
        if self.shapes.remove(root).is_some() {
            self.ids_changed_frame = frame;
        }
    }

    pub fn contains(&self, id: &SlotShapeId) -> bool {
        self.shapes.contains_key(id)
    }

    pub fn get(&self, id: &SlotShapeId) -> Option<&SlotShape> {
        self.shapes.get(id).map(|entry| &entry.node)
    }

    pub fn entry(&self, id: &SlotShapeId) -> Option<&VersionedSlotShape> {
        self.shapes.get(id)
    }

    pub fn snapshot(&self) -> SlotShapeRegistrySnapshot {
        SlotShapeRegistrySnapshot {
            ids_changed_frame: self.ids_changed_frame,
            shapes: self.shapes.clone(),
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_changed_frame = snapshot.ids_changed_frame;
        self.shapes = snapshot.shapes;
    }

    /// Validate a dynamic slot tree against this registry.
    pub fn validate_tree(&self, tree: &SlotTree) -> Result<(), SlotValidationError> {
        tree.validate(self)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistrySnapshot {
    pub ids_changed_frame: FrameId,
    pub shapes: BTreeMap<SlotShapeId, VersionedSlotShape>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotShapeRegistryError {
    DuplicateShapeId(SlotShapeId),
    ShapeIdConflict(SlotShapeId),
    MissingReferencedShape(SlotShapeId),
}

impl core::fmt::Display for SlotShapeRegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateShapeId(id) => write!(f, "duplicate slot shape id: {id}"),
            Self::ShapeIdConflict(id) => write!(f, "conflicting slot shape id: {id}"),
            Self::MissingReferencedShape(id) => {
                write!(f, "missing referenced slot shape id: {id}")
            }
        }
    }
}

impl core::error::Error for SlotShapeRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelType, SlotFieldShape, SlotMapKeyShape, SlotMeta, SlotVariantShape};
    use alloc::boxed::Box;
    use alloc::vec;

    #[test]
    fn ensure_tree_inserts_new_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        let inserted = registry
            .ensure_tree(id, SlotShape::value(ModelType::Bool))
            .unwrap();

        assert!(inserted);
        assert!(registry.contains(&id));
    }

    #[test]
    fn ensure_tree_is_idempotent_for_same_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");
        let shape = SlotShape::value(ModelType::Bool);

        assert!(registry.ensure_tree(id, shape.clone()).unwrap());
        assert!(!registry.ensure_tree(id, shape).unwrap());
    }

    #[test]
    fn ensure_tree_rejects_conflicting_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        registry
            .ensure_tree(id, SlotShape::value(ModelType::Bool))
            .unwrap();
        let err = registry
            .ensure_tree(id, SlotShape::value(ModelType::F32))
            .unwrap_err();

        assert_eq!(err, SlotShapeRegistryError::ShapeIdConflict(id));
    }

    #[test]
    fn referenced_shape_ids_collects_nested_refs() {
        let first = SlotShapeId::from_static_name("first");
        let second = SlotShapeId::from_static_name("second");
        let third = SlotShapeId::from_static_name("third");
        let fourth = SlotShapeId::from_static_name("fourth");
        let shape = SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                SlotFieldShape::new("field", SlotShape::reference(first)).unwrap(),
                SlotFieldShape::new(
                    "map",
                    SlotShape::Map {
                        meta: SlotMeta::empty(),
                        key: SlotMapKeyShape::String,
                        value: Box::new(SlotShape::reference(second)),
                    },
                )
                .unwrap(),
                SlotFieldShape::new(
                    "choice",
                    SlotShape::Enum {
                        meta: SlotMeta::empty(),
                        variants: vec![
                            SlotVariantShape::new(
                                "variant",
                                SlotShape::Option {
                                    meta: SlotMeta::empty(),
                                    some: Box::new(SlotShape::reference(third)),
                                },
                            )
                            .unwrap(),
                        ],
                    },
                )
                .unwrap(),
                SlotFieldShape::new("again", SlotShape::reference(fourth)).unwrap(),
            ],
        };

        assert_eq!(
            shape.referenced_shape_ids(),
            vec![first, second, third, fourth]
        );
    }
}
