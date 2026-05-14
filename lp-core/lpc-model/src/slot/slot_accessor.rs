//! Compiled access to a slot path.
//!
//! [`SlotPath`](crate::SlotPath) is the authored and wire-facing address. A
//! [`SlotAccessor`] is the runtime form compiled against a
//! [`SlotShapeRegistry`](crate::SlotShapeRegistry): record field names have
//! already been resolved to field indexes, and the accessor is guarded by the
//! registry revision it was compiled from.

use crate::slot::SlotReadContext;
use crate::{
    FromLpValue, Revision, SlotAccess, SlotDataAccess, SlotName, SlotPath, SlotPathSegment,
    SlotShape, SlotShapeId, SlotShapeRegistry,
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Indexed, registry-revision-checked access to one slot path.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SlotAccessor {
    root: SlotShapeId,
    registry_revision: Revision,
    path: SlotPath,
    steps: Vec<SlotAccessorStep>,
}

/// One compiled step through a slot tree.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SlotAccessorStep {
    /// Record field access by stable field index.
    RecordField { index: usize, name: SlotName },
    /// Option payload access through the conventional `some` field.
    OptionSome,
}

/// Error returned while compiling or using a [`SlotAccessor`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotAccessorError {
    message: String,
}

impl SlotAccessor {
    /// Compile a semantic path into an indexed accessor.
    pub fn compile(
        root: SlotShapeId,
        path: SlotPath,
        registry: &SlotShapeRegistry,
    ) -> Result<Self, SlotAccessorError> {
        let shape = registry.get(&root).ok_or_else(|| {
            SlotAccessorError::new(format!("missing slot path root shape {root}"))
        })?;
        let mut shape = resolve_ref_shape(shape, registry)?;
        let mut steps = Vec::new();

        for segment in path.segments() {
            match (shape, segment) {
                (SlotShape::Record { fields, .. }, SlotPathSegment::Field(name)) => {
                    let (index, field) = fields
                        .iter()
                        .enumerate()
                        .find(|(_, field)| field.name == *name)
                        .ok_or_else(|| {
                            SlotAccessorError::new(format!("record has no field {name}"))
                        })?;
                    steps.push(SlotAccessorStep::RecordField {
                        index,
                        name: name.clone(),
                    });
                    shape = resolve_ref_shape(&field.shape, registry)?;
                }
                (SlotShape::Option { some, .. }, SlotPathSegment::Field(name))
                    if name.as_str() == "some" =>
                {
                    steps.push(SlotAccessorStep::OptionSome);
                    shape = resolve_ref_shape(some, registry)?;
                }
                (_, SlotPathSegment::Field(name)) => {
                    return Err(SlotAccessorError::new(format!(
                        "slot path field {name} cannot descend into current slot shape"
                    )));
                }
                (_, SlotPathSegment::Key(key)) => {
                    return Err(SlotAccessorError::new(format!(
                        "compiled map-key slot access is not implemented yet: {key:?}"
                    )));
                }
            }
        }

        Ok(Self {
            root,
            registry_revision: registry.revision(),
            path,
            steps,
        })
    }

    /// Compile and require that the path lands on a value leaf.
    pub fn compile_value(
        root: SlotShapeId,
        path: SlotPath,
        registry: &SlotShapeRegistry,
    ) -> Result<Self, SlotAccessorError> {
        let accessor = Self::compile(root, path, registry)?;
        let shape = accessor.leaf_shape(registry)?;
        if !matches!(shape, SlotShape::Value { .. }) {
            return Err(SlotAccessorError::new(format!(
                "slot path {} does not resolve to a value leaf",
                accessor.path
            )));
        }
        Ok(accessor)
    }

    /// Root shape id this accessor was compiled against.
    pub fn root(&self) -> SlotShapeId {
        self.root
    }

    /// Registry revision this accessor was compiled against.
    pub fn registry_revision(&self) -> Revision {
        self.registry_revision
    }

    /// Original semantic path, kept for diagnostics and resolver compatibility.
    pub fn path(&self) -> &SlotPath {
        &self.path
    }

    /// Resolve this accessor as a typed value through a runtime context.
    ///
    /// Generated views usually return [`crate::SlotFieldReader`] wrappers, but
    /// this keeps hand-authored nested accessors ergonomic while they still
    /// exist.
    pub fn get<C, T>(&self, ctx: &mut C) -> Result<T, C::Error>
    where
        C: SlotReadContext,
        T: FromLpValue,
    {
        ctx.read_slot_value(self)
    }

    /// Access borrowed slot data using compiled indexes.
    pub fn access<'a>(
        &self,
        root: &'a dyn SlotAccess,
        registry: &SlotShapeRegistry,
    ) -> Result<SlotDataAccess<'a>, SlotAccessorError> {
        self.check_registry_revision(registry)?;
        if root.shape_id() != self.root {
            return Err(SlotAccessorError::new(format!(
                "slot accessor path root {} does not match data shape {}",
                self.root,
                root.shape_id()
            )));
        }

        let mut data = root.data();
        for step in &self.steps {
            match (step, data) {
                (SlotAccessorStep::RecordField { index, name }, SlotDataAccess::Record(record)) => {
                    data = record.field(*index).ok_or_else(|| {
                        SlotAccessorError::new(format!("record field {name} has no data"))
                    })?;
                }
                (SlotAccessorStep::RecordField { name, .. }, _) => {
                    return Err(SlotAccessorError::new(format!(
                        "slot path field {name} cannot descend into current slot data"
                    )));
                }
                (SlotAccessorStep::OptionSome, SlotDataAccess::Option(option)) => {
                    data = option
                        .data()
                        .ok_or_else(|| SlotAccessorError::new("option slot is none"))?;
                }
                (SlotAccessorStep::OptionSome, _) => {
                    return Err(SlotAccessorError::new(
                        "slot path field some cannot descend into current slot data",
                    ));
                }
            }
        }
        Ok(data)
    }

    fn check_registry_revision(
        &self,
        registry: &SlotShapeRegistry,
    ) -> Result<(), SlotAccessorError> {
        if self.registry_revision == registry.revision() {
            Ok(())
        } else {
            Err(SlotAccessorError::new(format!(
                "slot accessor for {} was compiled at registry revision {:?}, current revision is {:?}",
                self.path,
                self.registry_revision,
                registry.revision()
            )))
        }
    }

    fn leaf_shape<'a>(
        &self,
        registry: &'a SlotShapeRegistry,
    ) -> Result<&'a SlotShape, SlotAccessorError> {
        self.check_registry_revision(registry)?;
        let mut shape = registry.get(&self.root).ok_or_else(|| {
            SlotAccessorError::new(format!("missing slot path root shape {}", self.root))
        })?;
        shape = resolve_ref_shape(shape, registry)?;
        for step in &self.steps {
            match (shape, step) {
                (SlotShape::Record { fields, .. }, SlotAccessorStep::RecordField { index, .. }) => {
                    let field = fields.get(*index).ok_or_else(|| {
                        SlotAccessorError::new(format!(
                            "compiled field index {index} is outside current shape"
                        ))
                    })?;
                    shape = resolve_ref_shape(&field.shape, registry)?;
                }
                (SlotShape::Option { some, .. }, SlotAccessorStep::OptionSome) => {
                    shape = resolve_ref_shape(some, registry)?;
                }
                _ => {
                    return Err(SlotAccessorError::new(
                        "compiled accessor no longer matches current shape",
                    ));
                }
            }
        }
        Ok(shape)
    }
}

impl SlotAccessorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for SlotAccessorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for SlotAccessorError {}

fn resolve_ref_shape<'a>(
    mut shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
) -> Result<&'a SlotShape, SlotAccessorError> {
    while let SlotShape::Ref { id } = shape {
        shape = registry
            .get(id)
            .ok_or_else(|| SlotAccessorError::new(format!("missing referenced slot shape {id}")))?;
    }
    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::SlotAccessor;
    use crate::{
        OptionSlot, SlotDataAccess, SlotPath, SlotShapeRegistry, StaticSlotShape, ValueSlot,
        lookup_slot_data,
    };

    #[test]
    fn compile_value_can_descend_into_option_some_payload() {
        let mut registry = SlotShapeRegistry::default();
        crate::slot_shapes::register_all_static_slot_shapes(&mut registry)
            .expect("static slot shapes");
        OptionRoot::ensure_registered(&mut registry).expect("option shape");

        let accessor = SlotAccessor::compile_value(
            OptionRoot::SHAPE_ID,
            SlotPath::parse("item.some").unwrap(),
            &registry,
        )
        .expect("item.some accessor");

        let root = OptionRoot {
            item: OptionSlot::some(ValueSlot::new(64_u32)),
        };
        let data = accessor.access(&root, &registry).expect("access data");
        assert!(matches!(
            data,
            SlotDataAccess::Value(value) if value.value() == crate::LpValue::U32(64)
        ));

        let lookup = lookup_slot_data(&root, &registry, &SlotPath::parse("item.some").unwrap())
            .expect("lookup data");
        assert!(matches!(
            lookup,
            SlotDataAccess::Value(value) if value.value() == crate::LpValue::U32(64)
        ));
    }

    #[derive(crate::SlotRecord)]
    struct OptionRoot {
        pub item: OptionSlot<ValueSlot<u32>>,
    }
}
