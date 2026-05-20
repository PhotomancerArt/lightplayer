//! Registry for id-addressed slot shapes.
//!
//! Static Rust-authored shapes and dynamic runtime-generated shapes both
//! register here. The registry is versioned so clients can sync shape
//! additions, removals, and replacements before applying slot data patches.

use crate::{
    Revision, SlotData, SlotFactory, SlotFactoryError, SlotMutAccess, SlotShape, SlotShapeId,
    current_revision,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Registry of id-addressed slot shapes.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistry {
    pub ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
    #[serde(skip)]
    #[cfg_attr(feature = "schema-gen", schemars(skip))]
    factories: BTreeMap<SlotShapeId, SlotFactory>,
}

/// Versioned registry entry for one slot shape.
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
    /// Register a new shape.
    ///
    /// This is intentionally strict: registering an id twice is an error even
    /// when the shape is identical. Use [`Self::ensure_shape`] for static shape
    /// bootstrap code that may be called more than once.
    pub fn register_shape(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_dynamic_shape(id, shape)
    }

    pub fn register_shape_named(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_dynamic_shape_named(id, name, shape)
    }

    pub fn register_dynamic_shape(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_with_factory(id, shape, SlotFactory::dynamic())
    }

    pub fn register_dynamic_shape_named(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_named_with_factory(id, name, shape, SlotFactory::dynamic())
    }

    pub fn register_uncreatable_shape(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_with_factory(id, shape, SlotFactory::unsupported())
    }

    pub fn register_shape_with_factory(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_with_version_and_factory(current_revision(), id, shape, factory)
    }

    pub fn register_shape_named_with_factory(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_named_with_version_and_factory(
            current_revision(),
            id,
            name,
            shape,
            factory,
        )
    }

    pub fn register_shape_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_with_version_and_factory(revision, id, shape, SlotFactory::dynamic())
    }

    pub fn register_shape_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&id) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(id));
        }
        self.shapes.insert(id, SlotShapeEntry::new(revision, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
        Ok(())
    }

    pub fn register_shape_named_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_shape_named_with_version_and_factory(
            revision,
            id,
            name,
            shape,
            SlotFactory::dynamic(),
        )
    }

    pub fn register_shape_named_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&id) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(id));
        }
        self.shapes
            .insert(id, SlotShapeEntry::named(revision, name, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
        Ok(())
    }

    /// Ensure that a static shape is present.
    ///
    /// Returns `Ok(true)` when the shape was inserted and `Ok(false)` when the
    /// same shape was already registered. If the id is already registered with
    /// a different shape, this returns a conflict error rather than replacing
    /// the existing shape.
    pub fn ensure_shape(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_dynamic_shape(id, shape)
    }

    pub fn ensure_shape_named(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_dynamic_shape_named(id, name, shape)
    }

    pub fn ensure_dynamic_shape(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_with_factory(id, shape, SlotFactory::dynamic())
    }

    pub fn ensure_dynamic_shape_named(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_named_with_factory(id, name, shape, SlotFactory::dynamic())
    }

    pub fn ensure_shape_with_factory(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_with_version_and_factory(current_revision(), id, shape, factory)
    }

    pub fn ensure_shape_named_with_factory(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_named_with_version_and_factory(
            current_revision(),
            id,
            name,
            shape,
            factory,
        )
    }

    pub fn ensure_shape_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_with_version_and_factory(revision, id, shape, SlotFactory::dynamic())
    }

    pub fn ensure_shape_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<bool, SlotShapeRegistryError> {
        if let Some(existing) = self.shapes.get(&id) {
            return if existing.value() == &shape {
                self.factories.insert(id, factory);
                Ok(false)
            } else {
                Err(SlotShapeRegistryError::ShapeIdConflict(id))
            };
        }

        self.shapes.insert(id, SlotShapeEntry::new(revision, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
        Ok(true)
    }

    pub fn ensure_shape_named_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) -> Result<bool, SlotShapeRegistryError> {
        self.ensure_shape_named_with_version_and_factory(
            revision,
            id,
            name,
            shape,
            SlotFactory::dynamic(),
        )
    }

    pub fn ensure_shape_named_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
        factory: SlotFactory,
    ) -> Result<bool, SlotShapeRegistryError> {
        let name = name.into();
        if let Some(existing) = self.shapes.get(&id) {
            return if existing.value() == &shape {
                self.factories.insert(id, factory);
                Ok(false)
            } else {
                Err(SlotShapeRegistryError::ShapeIdConflict(id))
            };
        }

        self.shapes
            .insert(id, SlotShapeEntry::named(revision, name, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
        Ok(true)
    }

    /// Replace a dynamic shape.
    ///
    /// Runtime-owned shapes whose structure varies by artifact or instance use
    /// this path when their shape changes.
    pub fn replace_shape(&mut self, id: SlotShapeId, shape: SlotShape) {
        self.replace_dynamic_shape(id, shape);
    }

    pub fn replace_shape_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
    ) {
        self.replace_shape_with_version_and_factory(revision, id, shape, SlotFactory::dynamic());
    }

    pub fn replace_dynamic_shape(&mut self, id: SlotShapeId, shape: SlotShape) {
        self.replace_shape_with_factory(id, shape, SlotFactory::dynamic());
    }

    pub fn replace_shape_with_factory(
        &mut self,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) {
        self.replace_shape_with_version_and_factory(current_revision(), id, shape, factory);
    }

    pub fn replace_shape_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        shape: SlotShape,
        factory: SlotFactory,
    ) {
        self.shapes.insert(id, SlotShapeEntry::new(revision, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
    }

    pub fn replace_shape_named(
        &mut self,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) {
        self.replace_shape_named_with_version(current_revision(), id, name, shape);
    }

    pub fn replace_shape_named_with_version(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
    ) {
        self.replace_shape_named_with_version_and_factory(
            revision,
            id,
            name,
            shape,
            SlotFactory::dynamic(),
        );
    }

    pub fn replace_shape_named_with_version_and_factory(
        &mut self,
        revision: Revision,
        id: SlotShapeId,
        name: impl Into<String>,
        shape: SlotShape,
        factory: SlotFactory,
    ) {
        self.shapes
            .insert(id, SlotShapeEntry::named(revision, name, shape));
        self.factories.insert(id, factory);
        self.ids_revision = revision;
    }

    pub fn unregister_shape(&mut self, id: &SlotShapeId) {
        self.unregister_shape_with_version(current_revision(), id);
    }

    pub fn unregister_shape_with_version(&mut self, revision: Revision, id: &SlotShapeId) {
        if self.shapes.remove(id).is_some() {
            self.factories.remove(id);
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

    pub fn iter(&self) -> impl Iterator<Item = (&SlotShapeId, &SlotShapeEntry)> {
        self.shapes.iter()
    }

    pub fn snapshot(&self) -> SlotShapeRegistrySnapshot {
        SlotShapeRegistrySnapshot {
            ids_revision: self.ids_revision,
            shapes: self.shapes.clone(),
        }
    }

    pub fn snapshot_page(
        &self,
        after: Option<SlotShapeId>,
        limit: usize,
    ) -> (SlotShapeRegistrySnapshot, Option<SlotShapeId>) {
        let mut shapes = BTreeMap::new();
        let mut last_included = None;
        let mut next = None;
        let limit = limit.max(1);
        let iter = self
            .shapes
            .iter()
            .filter(|(id, _)| after.is_none_or(|after| **id > after));
        for (id, entry) in iter {
            if shapes.len() >= limit {
                next = last_included;
                break;
            }
            shapes.insert(*id, entry.clone());
            last_included = Some(*id);
        }
        (
            SlotShapeRegistrySnapshot {
                ids_revision: self.ids_revision,
                shapes,
            },
            next,
        )
    }

    pub fn apply_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_revision = snapshot.ids_revision;
        self.shapes = snapshot.shapes;
        self.factories = self
            .shapes
            .keys()
            .map(|id| (*id, SlotFactory::unsupported()))
            .collect();
    }

    pub fn apply_partial_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_revision = snapshot.ids_revision;
        for (id, entry) in snapshot.shapes {
            self.shapes.insert(id, entry);
            self.factories
                .entry(id)
                .or_insert_with(SlotFactory::unsupported);
        }
    }

    pub fn create_default(
        &self,
        id: SlotShapeId,
    ) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError> {
        if !self.shapes.contains_key(&id) {
            return Err(SlotFactoryError::MissingShape(id));
        }
        self.factories
            .get(&id)
            .copied()
            .unwrap_or_else(SlotFactory::unsupported)
            .create_default(self, id)
    }

    pub fn read_slot_json(
        &self,
        id: SlotShapeId,
        json: &str,
    ) -> Result<Box<dyn SlotMutAccess>, crate::slot_codec::SyntaxError> {
        self.read_slot_from(id, crate::slot_codec::JsonSyntaxSource::new(json)?)
    }

    pub fn read_slot_json_data(
        &self,
        id: SlotShapeId,
        json: &str,
    ) -> Result<SlotData, crate::slot_codec::SyntaxError> {
        let mut reader = crate::slot_codec::SlotReader::new(
            crate::slot_codec::JsonSyntaxSource::new(json)?,
            self,
        );
        crate::slot_codec::read_dynamic_slot_data(self, id, reader.value())
    }

    pub fn read_slot_toml(
        &self,
        id: SlotShapeId,
        value: &toml::Value,
    ) -> Result<Box<dyn SlotMutAccess>, crate::slot_codec::SyntaxError> {
        self.read_slot_from(id, crate::slot_codec::TomlSyntaxSource::new(value)?)
    }

    pub fn read_slot_from<S>(
        &self,
        id: SlotShapeId,
        source: S,
    ) -> Result<Box<dyn SlotMutAccess>, crate::slot_codec::SyntaxError>
    where
        S: crate::slot_codec::SyntaxEventSource,
    {
        let mut reader = crate::slot_codec::SlotReader::new(source, self);
        crate::slot_codec::read_dynamic_slot(self, id, reader.value())
    }

    pub fn write_slot_json<W>(
        &self,
        root: &dyn crate::SlotAccess,
        out: W,
    ) -> Result<W, crate::slot_codec::SlotWriteError<W::Error>>
    where
        W: crate::slot_codec::SlotWrite,
    {
        crate::slot_codec::write_dynamic_slot_json(self, root, out)
    }

    pub fn write_slot_json_value<W>(
        &self,
        id: SlotShapeId,
        data: crate::SlotDataAccess<'_>,
        value: crate::slot_codec::SlotValueWriter<'_, W>,
    ) -> Result<(), crate::slot_codec::SlotWriteError<W::Error>>
    where
        W: crate::slot_codec::SlotWrite,
    {
        crate::slot_codec::write_slot_data_json_value(self, id, data, value)
    }

    pub fn write_slot_toml(
        &self,
        root: &dyn crate::SlotAccess,
    ) -> Result<toml::Value, crate::slot_codec::SlotDataWriteError> {
        crate::slot_codec::write_dynamic_slot_toml(self, root)
    }

    pub fn write_slot_toml_data(
        &self,
        id: SlotShapeId,
        data: crate::SlotDataAccess<'_>,
    ) -> Result<toml::Value, crate::slot_codec::SlotDataWriteError> {
        crate::slot_codec::write_slot_data_toml_value(self, id, data)
    }
}

impl PartialEq for SlotShapeRegistry {
    fn eq(&self, other: &Self) -> bool {
        self.ids_revision == other.ids_revision && self.shapes == other.shapes
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
    FactoryError(String),
}

impl core::fmt::Display for SlotShapeRegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateShapeId(id) => write!(f, "duplicate slot shape id: {id}"),
            Self::ShapeIdConflict(id) => write!(f, "conflicting slot shape id: {id}"),
            Self::MissingReferencedShape(id) => {
                write!(f, "missing referenced slot shape id: {id}")
            }
            Self::FactoryError(message) => f.write_str(message),
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
    use alloc::vec::Vec;

    #[test]
    fn ensure_shape_inserts_new_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        let inserted = registry
            .ensure_shape(id, SlotShape::value(LpType::Bool))
            .unwrap();

        assert!(inserted);
        assert!(registry.contains(&id));
    }

    #[test]
    fn ensure_shape_is_idempotent_for_same_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");
        let shape = SlotShape::value(LpType::Bool);

        assert!(registry.ensure_shape(id, shape.clone()).unwrap());
        assert!(!registry.ensure_shape(id, shape).unwrap());
    }

    #[test]
    fn ensure_shape_rejects_conflicting_shape() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.shape");

        registry
            .ensure_shape(id, SlotShape::value(LpType::Bool))
            .unwrap();
        let err = registry
            .ensure_shape(id, SlotShape::value(LpType::F32))
            .unwrap_err();

        assert_eq!(err, SlotShapeRegistryError::ShapeIdConflict(id));
    }

    #[test]
    fn named_shape_preserves_debug_name_in_snapshot() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.named_shape");

        registry
            .ensure_shape_named(
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
    fn apply_snapshot_restores_shapes_as_explicitly_uncreatable() {
        let mut registry = SlotShapeRegistry::default();
        let id = SlotShapeId::from_static_name("test.snapshot_shape");
        registry
            .register_dynamic_shape(id, SlotShape::value(LpType::Bool))
            .unwrap();
        let snapshot = registry.snapshot();

        let mut restored = SlotShapeRegistry::default();
        restored.apply_snapshot(snapshot);

        let Err(error) = restored.create_default(id) else {
            panic!("expected unsupported factory");
        };
        assert_eq!(error, SlotFactoryError::UnsupportedFactory(id));
    }

    #[test]
    fn snapshot_page_cursor_collects_all_entries_with_limit_one() {
        let mut registry = SlotShapeRegistry::default();
        let ids = [
            SlotShapeId::new(10),
            SlotShapeId::new(20),
            SlotShapeId::new(30),
            SlotShapeId::new(40),
        ];
        for id in ids {
            registry
                .register_dynamic_shape(id, SlotShape::value(LpType::Bool))
                .unwrap();
        }

        let mut cursor = None;
        let mut collected = Vec::new();
        loop {
            let (snapshot, next) = registry.snapshot_page(cursor, 1);
            collected.extend(snapshot.shapes.keys().copied());
            if next.is_none() {
                break;
            }
            cursor = next;
        }

        assert_eq!(collected, ids);
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
                        encoding: crate::SlotEnumEncoding::default(),
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
