//! Registry for id-addressed slot shape roots.
//!
//! Static Rust-authored slot roots and dynamic runtime-generated roots both
//! register here. The registry is versioned so clients can sync shape additions,
//! removals, and replacements before applying slot data patches.

use crate::{Revision, SlotShape, SlotShapeId, current_revision};
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Registry of id-addressed slot shape roots.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistry {
    pub ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
}

/// Versioned registry entry for one slot shape root.
///
/// Shape ids are compact integers on the wire and in embedded lookup tables.
/// `name` preserves the human/debug name that produced the id when one is
/// known, so tools do not have to display only hash-like ids.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeEntry {
    pub changed_at: Revision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub shape: SlotShape,
}

impl SlotShapeEntry {
    pub fn new(changed_at: Revision, shape: SlotShape) -> Self {
        Self {
            changed_at,
            name: None,
            shape,
        }
    }

    pub fn named(changed_at: Revision, name: impl Into<String>, shape: SlotShape) -> Self {
        Self {
            changed_at,
            name: Some(name.into()),
            shape,
        }
    }

    pub fn changed_at(&self) -> Revision {
        self.changed_at
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn value(&self) -> &SlotShape {
        &self.shape
    }
}

impl SlotShapeRegistry {
    /// Register a new shape root.
    ///
    /// This is intentionally strict: registering an id twice is an error even
    /// when the shape is identical. Use [`Self::ensure_root`] for static shape
    /// bootstrap code that may be called more than once.
    pub fn register_root(
        &mut self,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_root_with_version(current_revision(), root, shape)
    }

    pub fn register_root_named(
        &mut self,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_root_named_with_version(current_revision(), root, name, shape)
    }

    pub fn register_root_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&root) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(root));
        }
        self.shapes
            .insert(root, SlotShapeEntry::new(revision, shape));
        self.ids_revision = revision;
        Ok(())
    }

    pub fn register_root_named_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&root) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(root));
        }
        self.shapes
            .insert(root, SlotShapeEntry::named(revision, name, shape));
        self.ids_revision = revision;
        Ok(())
    }

    /// Ensure that a static shape root is present.
    ///
    /// Returns `Ok(true)` when the shape was inserted and `Ok(false)` when the
    /// same shape was already registered. If the id is already registered with
    /// a different shape, this returns a conflict error rather than replacing
    /// the existing shape.
    pub fn ensure_root(
        &mut self,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_root_with_version(current_revision(), root, shape)
    }

    pub fn ensure_root_named(
        &mut self,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_root_named_with_version(current_revision(), root, name, shape)
    }

    pub fn ensure_root_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        if let Some(existing) = self.shapes.get(&root) {
            return if existing.value() == &shape {
                Ok(false)
            } else {
                Err(SlotShapeRegistryError::ShapeIdConflict(root))
            };
        }

        self.shapes
            .insert(root, SlotShapeEntry::new(revision, shape));
        self.ids_revision = revision;
        Ok(true)
    }

    pub fn ensure_root_named_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        let name = name.into();
        if let Some(existing) = self.shapes.get(&root) {
            return if existing.value() == &shape {
                Ok(false)
            } else {
                Err(SlotShapeRegistryError::ShapeIdConflict(root))
            };
        }

        self.shapes
            .insert(root, SlotShapeEntry::named(revision, name, shape));
        self.ids_revision = revision;
        Ok(true)
    }

    /// Replace a dynamic shape root.
    ///
    /// Runtime-owned shapes whose structure varies by artifact or instance use
    /// this path when their shape changes.
    pub fn replace_root(&mut self, root: SlotShapeId, shape: SlotShape) {
        self.replace_root_with_version(current_revision(), root, shape);
    }

    pub fn replace_root_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        shape: SlotShape,
    ) {
        self.shapes
            .insert(root, SlotShapeEntry::new(revision, shape));
        self.ids_revision = revision;
    }

    pub fn replace_root_named(
        &mut self,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) {
        self.replace_root_named_with_version(current_revision(), root, name, shape);
    }

    pub fn replace_root_named_with_version(
        &mut self,
        revision: Revision,
        root: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) {
        self.shapes
            .insert(root, SlotShapeEntry::named(revision, name, shape));
        self.ids_revision = revision;
    }

    pub fn unregister_root(&mut self, root: &SlotShapeId) {
        self.unregister_root_with_version(current_revision(), root);
    }

    pub fn unregister_root_with_version(&mut self, revision: Revision, root: &SlotShapeId) {
        if self.shapes.remove(root).is_some() {
            self.ids_revision = revision;
        }
    }

    pub fn contains(&self, id: &SlotShapeId) -> bool {
        self.shapes.contains_key(id)
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Current registry-wide revision for conservative accessor invalidation.
    pub fn revision(&self) -> Revision {
        self.ids_revision
    }

    pub fn get(&self, id: &SlotShapeId) -> Option<&SlotShape> {
        self.shapes.get(id).map(SlotShapeEntry::value)
    }

    pub fn entry(&self, id: &SlotShapeId) -> Option<&SlotShapeEntry> {
        self.shapes.get(id)
    }

    /// Find a registered root id by its human-readable shape name.
    ///
    /// Named lookup is for authoring and debug paths such as native shader slot
    /// references. Runtime/wire data should continue to use compact
    /// [`SlotShapeId`] values once a name has been resolved.
    pub fn id_for_name(&self, name: &str) -> Option<SlotShapeId> {
        self.shapes.iter().find_map(|(id, entry)| {
            if entry.name() == Some(name) {
                Some(*id)
            } else {
                None
            }
        })
    }

    /// Find a registered root entry by its human-readable shape name.
    pub fn entry_by_name(&self, name: &str) -> Option<(SlotShapeId, &SlotShapeEntry)> {
        self.shapes.iter().find_map(|(id, entry)| {
            if entry.name() == Some(name) {
                Some((*id, entry))
            } else {
                None
            }
        })
    }

    /// Find a registered root shape by its human-readable shape name.
    pub fn get_by_name(&self, name: &str) -> Option<&SlotShape> {
        self.entry_by_name(name).map(|(_, entry)| entry.value())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SlotShapeId, &SlotShapeEntry)> {
        self.shapes.iter()
    }

    pub fn snapshot(&self) -> SlotShapeRegistrySnapshot {
        SlotShapeRegistrySnapshot {
            ids_revision: self.ids_revision,
            shapes: self.shapes.clone(),
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_revision = snapshot.ids_revision;
        self.shapes = snapshot.shapes;
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistrySnapshot {
    pub ids_revision: Revision,
    pub shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
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
    use crate::{LpType, SlotFieldShape, SlotMapKeyShape, SlotMeta, SlotVariantShape};
    use alloc::boxed::Box;
    use alloc::vec;

    #[test]
    fn ensure_root_inserts_new_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        let inserted = registry
            .ensure_root(id, SlotShape::value(LpType::Bool))
            .unwrap();

        assert!(inserted);
        assert!(registry.contains(&id));
    }

    #[test]
    fn ensure_root_is_idempotent_for_same_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");
        let shape = SlotShape::value(LpType::Bool);

        assert!(registry.ensure_root(id, shape.clone()).unwrap());
        assert!(!registry.ensure_root(id, shape).unwrap());
    }

    #[test]
    fn ensure_root_rejects_conflicting_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        registry
            .ensure_root(id, SlotShape::value(LpType::Bool))
            .unwrap();
        let err = registry
            .ensure_root(id, SlotShape::value(LpType::F32))
            .unwrap_err();

        assert_eq!(err, SlotShapeRegistryError::ShapeIdConflict(id));
    }

    #[test]
    fn named_root_preserves_debug_name_in_snapshot() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.named_shape");

        registry
            .ensure_root_named(
                id,
                "crate::test::NamedShape",
                SlotShape::value(LpType::Bool),
            )
            .unwrap();

        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot.shapes.get(&id).and_then(SlotShapeEntry::name),
            Some("crate::test::NamedShape")
        );
    }

    #[test]
    fn named_root_can_be_resolved_by_name() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("lp::fluid::Emitter");
        let shape = SlotShape::value(LpType::Bool);

        registry
            .ensure_root_named(id, "lp::fluid::Emitter", shape.clone())
            .unwrap();

        assert_eq!(registry.id_for_name("lp::fluid::Emitter"), Some(id));
        assert_eq!(registry.get_by_name("lp::fluid::Emitter"), Some(&shape));
        assert!(registry.entry_by_name("lp::missing::Shape").is_none());
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
