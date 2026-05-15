//! Slot identity and value-reference model.
//!
//! A slot is a named location owned by a node or bus. A [`ValuePath`] navigates
//! inside the value exposed at that slot; it is not part of the slot identity.

mod slot_access;
mod slot_accessor;
mod slot_data;
mod slot_enum_shape;
mod slot_factory;
mod slot_lookup;
mod slot_meta;
mod slot_mut_access;
mod slot_mutation;
mod slot_name;
mod slot_owner;
mod slot_path;
mod slot_reader;
mod slot_record_shape;
mod slot_ref;
mod slot_shape;
mod slot_shape_builder;
mod slot_shape_registry;
mod slot_value;
mod value_ref;
mod value_slot;

pub use slot_access::{
    FieldSlot, MapSlotAccess, SlotAccess, SlotDataAccess, SlotEnumAccess, SlotOptionAccess,
    SlotRecordAccess, SlotValueAccess, StaticSlotAccess, StaticSlotShape,
};
pub use slot_accessor::{SlotAccessor, SlotAccessorError, SlotAccessorStep};
pub use slot_data::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};
pub use slot_enum_shape::SlotEnumShape;
pub use slot_factory::{
    DynamicSlotObject, SlotFactory, SlotFactoryError, SlotFactoryFn, create_dynamic_slot_data,
};
pub use slot_lookup::{SlotLookupError, lookup_slot_data};
pub use slot_meta::SlotMeta;
pub use slot_mut_access::{
    FieldSlotMut, MapSlotMutAccess, SlotDataMutAccess, SlotEnumDefaultVariant, SlotEnumMutAccess,
    SlotMapValueMutAccess, SlotMutAccess, SlotMutationError, SlotOptionMutAccess,
    SlotRecordMutAccess, SlotValueMutAccess,
};
pub use slot_mutation::{
    insert_slot_map_entry_default, set_slot_option_some_default, set_slot_value,
    set_slot_variant_default, slot_data_revision,
};
pub use slot_name::{SlotName, SlotNameError};
pub use slot_owner::SlotOwner;
pub use slot_path::{SlotPath, SlotPathError, SlotPathSegment};
pub use slot_reader::{SlotFieldReader, SlotOptionReader, SlotReadContext};
pub use slot_record_shape::SlotRecordShape;
pub use slot_ref::SlotRef;
pub use slot_shape::{
    SlotFieldShape, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeIdError, SlotVariantShape,
};
pub use slot_value::{
    FromLpValue, OrderedF32, SlotEnumOption, SlotValue, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError,
};
pub mod shape {
    pub use super::slot_shape_builder::{
        field, id, leaf, map, option, record, reference, unit, value, variant,
    };
}
pub use slot_shape_registry::{
    SlotShapeEntry, SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot,
};

pub use crate::slots::{
    Affine2d, Affine2dSlot, ArtifactPath, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue,
    ControlProductSlot, Dim2u, Dim2uSlot, PositiveF32, PositiveF32Slot, Ratio, RatioSlot,
    RelativeNodeRefSlot, RenderOrder, RenderOrderSlot, ResourceRefSlot, SourcePath, SourcePathSlot,
    VisualProductSlot, Xy, XySlot,
};
pub use value_ref::ValueRef;
pub use value_slot::{MapSlot, MapSlotKeyLike, OptionSlot, SlotMapValueAccess, ValueSlot};
