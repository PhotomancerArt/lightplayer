use crate::{
    LpValue, Revision, SlotShape, SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError,
    WithRevision,
};
use alloc::vec::Vec;

use super::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};

/// Runtime object that exposes slot-addressable data.
///
/// Artifacts, node definitions, runtime nodes, state structs, and dynamic
/// records can all expose slot data. The object carries the shape id; walking
/// below it pairs data access with shape information from the shape registry.
pub trait SlotAccess {
    fn shape_id(&self) -> SlotShapeId;
    fn data(&self) -> SlotDataAccess<'_>;
}

/// Static slot shape authored by a Rust type.
///
/// Static shapes are type-owned descriptions, not per-instance data. They are
/// appropriate for Rust-authored defs, configs, and fixed runtime state whose
/// structure does not vary by loaded artifact. Dynamic shapes, such as shader
/// params authored by a specific shader file, should be registered by their
/// runtime owner with an instance- or artifact-specific id instead.
pub trait StaticSlotShape {
    const SHAPE_ID: SlotShapeId;

    fn slot_shape() -> SlotShape;

    fn shape_name() -> Option<&'static str> {
        None
    }

    fn ensure_registered(registry: &mut SlotShapeRegistry) -> Result<bool, SlotShapeRegistryError> {
        match Self::shape_name() {
            Some(name) => registry.ensure_shape_named(Self::SHAPE_ID, name, Self::slot_shape()),
            None => registry.ensure_shape(Self::SHAPE_ID, Self::slot_shape()),
        }
    }
}

/// Slot-accessible object whose data and shape are both authored statically by Rust.
///
/// This is the data-access counterpart to [`StaticSlotShape`]. It remains as
/// the ergonomic trait for code that needs both a runtime value and its static
/// shape identity. `register_shape` is kept as a compatibility shim for older
/// call sites; new static bootstrap code should prefer `ensure_registered`.
pub trait StaticSlotAccess: SlotAccess + StaticSlotShape {
    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        Self::ensure_registered(registry).map(|_| ())
    }
}

/// Field-level slot access used by derive inference.
///
/// A record field that implements this trait can be included in
/// `#[derive(SlotRecord)]` without an explicit shape attribute. Fields that do
/// not implement this trait must provide an explicit override supported by the
/// derive or use a custom slot-access implementation.
pub trait FieldSlot {
    fn slot_field_shape() -> SlotShape;
    fn slot_field_data(&self) -> SlotDataAccess<'_>;
}

/// Borrowed access to one slot-data node.
#[derive(Clone, Copy)]
pub enum SlotDataAccess<'a> {
    Unit(Revision),
    Value(&'a dyn SlotValueAccess),
    Record(&'a dyn SlotRecordAccess),
    Map(&'a dyn MapSlotAccess),
    Enum(&'a dyn SlotEnumAccess),
    Option(&'a dyn SlotOptionAccess),
}

/// Borrowed access to an atomic slot value.
pub trait SlotValueAccess {
    fn changed_at(&self) -> Revision;
    fn value(&self) -> LpValue;
}

/// Borrowed access to a record slot.
pub trait SlotRecordAccess {
    fn fields_revision(&self) -> Revision {
        Revision::default()
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>>;
}

/// Borrowed access to a stable-key map slot.
pub trait MapSlotAccess {
    fn keys_revision(&self) -> Revision;
    fn keys(&self) -> Vec<SlotMapKey>;
    fn get(&self, key: &SlotMapKey) -> Option<SlotDataAccess<'_>>;
}

/// Borrowed access to an enum slot with one active variant.
pub trait SlotEnumAccess {
    fn variant_revision(&self) -> Revision;
    fn variant(&self) -> &str;
    fn data(&self) -> SlotDataAccess<'_>;
}

/// Borrowed access to an optional slot.
pub trait SlotOptionAccess {
    fn presence_revision(&self) -> Revision;
    fn data(&self) -> Option<SlotDataAccess<'_>>;
}

impl SlotData {
    pub fn access(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Unit { revision } => SlotDataAccess::Unit(*revision),
            Self::Value(value) => SlotDataAccess::Value(value),
            Self::Record(record) => SlotDataAccess::Record(record),
            Self::Map(map) => SlotDataAccess::Map(map),
            Self::Enum(en) => SlotDataAccess::Enum(en),
            Self::Option(option) => SlotDataAccess::Option(option),
        }
    }
}

impl SlotValueAccess for WithRevision<LpValue> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        self.value().clone()
    }
}

impl SlotValueAccess for WithRevision<f32> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::F32(*self.value())
    }
}

impl SlotValueAccess for WithRevision<u32> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::U32(*self.value())
    }
}

impl SlotValueAccess for WithRevision<bool> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::Bool(*self.value())
    }
}

impl SlotValueAccess for WithRevision<[f32; 2]> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::Vec2(*self.value())
    }
}

impl SlotValueAccess for WithRevision<[f32; 3]> {
    fn changed_at(&self) -> Revision {
        self.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::Vec3(*self.value())
    }
}

impl SlotRecordAccess for SlotRecord {
    fn fields_revision(&self) -> Revision {
        self.fields_revision
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        self.fields.get(index).map(SlotData::access)
    }
}

impl MapSlotAccess for SlotMapDyn {
    fn keys_revision(&self) -> Revision {
        self.keys_revision
    }

    fn keys(&self) -> Vec<SlotMapKey> {
        self.entries.keys().cloned().collect()
    }

    fn get(&self, key: &SlotMapKey) -> Option<SlotDataAccess<'_>> {
        self.entries.get(key).map(SlotData::access)
    }
}

impl SlotEnumAccess for SlotEnum {
    fn variant_revision(&self) -> Revision {
        self.variant_revision
    }

    fn variant(&self) -> &str {
        self.variant.as_str()
    }

    fn data(&self) -> SlotDataAccess<'_> {
        self.data.access()
    }
}

impl SlotOptionAccess for SlotOptionDyn {
    fn presence_revision(&self) -> Revision {
        self.presence_revision
    }

    fn data(&self) -> Option<SlotDataAccess<'_>> {
        self.data.as_ref().map(|data| data.access())
    }
}
