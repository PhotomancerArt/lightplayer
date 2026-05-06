use crate::{
    FrameId, ModelValue, SlotShape, SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError,
    Versioned,
};
use alloc::vec::Vec;

use super::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};

/// Root object that exposes slot-addressable data.
///
/// Artifacts, node definitions, runtime nodes, state structs, and dynamic
/// records can all expose a slot root. The root carries the shape id; walking
/// below it pairs data access with shape information from the shape registry.
pub trait SlotAccess {
    fn shape_id(&self) -> SlotShapeId;
    fn data(&self) -> SlotDataAccess<'_>;
}

/// Slot root whose shape is authored statically by its Rust implementation.
///
/// Static slot roots do not store shape identity per value. The type owns a
/// stable numeric shape id and knows how to register its shape into a registry
/// during startup.
pub trait StaticSlotAccess: SlotAccess {
    const SHAPE_ID: SlotShapeId;

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError>;
}

/// Field-level slot access used by derive inference.
///
/// A record field that implements this trait can be included in
/// `#[derive(SlotRecord)]` without an explicit shape attribute. Fields that do
/// not implement this trait must opt out with `#[slot(skip)]` or provide an
/// explicit override supported by the derive.
pub trait FieldSlot {
    fn slot_field_shape() -> SlotShape;
    fn slot_field_data(&self) -> SlotDataAccess<'_>;
}

/// Borrowed access to one slot-data node.
#[derive(Clone, Copy)]
pub enum SlotDataAccess<'a> {
    Unit(FrameId),
    Value(&'a dyn SlotValueAccess),
    Record(&'a dyn SlotRecordAccess),
    Map(&'a dyn MapSlotAccess),
    Enum(&'a dyn SlotEnumAccess),
    Option(&'a dyn SlotOptionAccess),
}

/// Borrowed access to an atomic slot value.
pub trait SlotValueAccess {
    fn changed_frame(&self) -> FrameId;
    fn value(&self) -> ModelValue;
}

/// Borrowed access to a record slot.
pub trait SlotRecordAccess {
    fn fields_changed_frame(&self) -> FrameId {
        FrameId::default()
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>>;
}

/// Borrowed access to a stable-key map slot.
pub trait MapSlotAccess {
    fn keys_changed_frame(&self) -> FrameId;
    fn keys(&self) -> Vec<SlotMapKey>;
    fn get(&self, key: &SlotMapKey) -> Option<SlotDataAccess<'_>>;
}

/// Borrowed access to an enum slot with one active variant.
pub trait SlotEnumAccess {
    fn variant_changed_frame(&self) -> FrameId;
    fn variant(&self) -> &str;
    fn data(&self) -> SlotDataAccess<'_>;
}

/// Borrowed access to an optional slot.
pub trait SlotOptionAccess {
    fn presence_changed_frame(&self) -> FrameId;
    fn data(&self) -> Option<SlotDataAccess<'_>>;
}

impl SlotData {
    pub fn access(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Unit { changed_frame } => SlotDataAccess::Unit(*changed_frame),
            Self::Value(value) => SlotDataAccess::Value(value),
            Self::Record(record) => SlotDataAccess::Record(record),
            Self::Map(map) => SlotDataAccess::Map(map),
            Self::Enum(en) => SlotDataAccess::Enum(en),
            Self::Option(option) => SlotDataAccess::Option(option),
        }
    }
}

impl SlotValueAccess for Versioned<ModelValue> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.value().clone()
    }
}

impl SlotValueAccess for Versioned<f32> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::F32(*self.value())
    }
}

impl SlotValueAccess for Versioned<u32> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::U32(*self.value())
    }
}

impl SlotValueAccess for Versioned<bool> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::Bool(*self.value())
    }
}

impl SlotValueAccess for Versioned<[f32; 2]> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::Vec2(*self.value())
    }
}

impl SlotValueAccess for Versioned<[f32; 3]> {
    fn changed_frame(&self) -> FrameId {
        self.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::Vec3(*self.value())
    }
}

impl SlotRecordAccess for SlotRecord {
    fn fields_changed_frame(&self) -> FrameId {
        self.fields_changed_frame
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        self.fields.get(index).map(SlotData::access)
    }
}

impl MapSlotAccess for SlotMapDyn {
    fn keys_changed_frame(&self) -> FrameId {
        self.keys_changed_frame
    }

    fn keys(&self) -> Vec<SlotMapKey> {
        self.entries.keys().cloned().collect()
    }

    fn get(&self, key: &SlotMapKey) -> Option<SlotDataAccess<'_>> {
        self.entries.get(key).map(SlotData::access)
    }
}

impl SlotEnumAccess for SlotEnum {
    fn variant_changed_frame(&self) -> FrameId {
        self.variant_changed_frame
    }

    fn variant(&self) -> &str {
        self.variant.as_str()
    }

    fn data(&self) -> SlotDataAccess<'_> {
        self.data.access()
    }
}

impl SlotOptionAccess for SlotOptionDyn {
    fn presence_changed_frame(&self) -> FrameId {
        self.presence_changed_frame
    }

    fn data(&self) -> Option<SlotDataAccess<'_>> {
        self.data.as_ref().map(|data| data.access())
    }
}
