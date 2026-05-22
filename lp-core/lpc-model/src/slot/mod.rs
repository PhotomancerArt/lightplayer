//! Slot identity and value-reference model.
//!
//! A slot is a named location owned by a node or bus. A [`ValuePath`] navigates
//! inside the value exposed at that slot; it is not part of the slot identity.

mod enum_slot;
mod slot_access;
mod slot_accessor;
mod slot_data;
mod slot_direction;
mod slot_enum_shape;
mod slot_factory;
mod slot_lookup;
mod slot_merge;
mod slot_meta;
mod slot_mut_access;
mod slot_mutation;
mod slot_name;
mod slot_owner;
mod slot_path;
mod slot_persistence;
mod slot_policy;
mod slot_reader;
mod slot_record_shape;
mod slot_ref;
mod slot_semantics;
mod slot_shape;
mod slot_shape_builder;
mod slot_shape_lookup;
mod slot_shape_registry;
mod slot_shape_view;
mod slot_value;
mod stable_hash;
mod static_slot_shape;
mod value_ref;
mod value_slot;

pub use enum_slot::{EnumSlot, SlottedEnum, SlottedEnumMut};
pub use slot_access::{
    FieldSlot, MapSlotAccess, SlotAccess, SlotCustomAccess, SlotDataAccess, SlotEnumAccess,
    SlotOptionAccess, SlotRecordAccess, SlotValueAccess, StaticSlotAccess, StaticSlotShape,
};
pub use slot_accessor::{SlotAccessor, SlotAccessorError, SlotAccessorStep};
pub use slot_data::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};
pub use slot_direction::SlotDirection;
pub use slot_enum_shape::SlotEnumShape;
pub use slot_factory::{
    DynamicSlotObject, SlotFactory, SlotFactoryError, SlotFactoryFn, create_dynamic_slot_data,
};
pub use slot_lookup::{
    SlotLookupError, lookup_slot_data, lookup_slot_data_and_shape, lookup_slot_data_mut,
};
pub use slot_merge::SlotMerge;
pub use slot_meta::SlotMeta;
pub use slot_mut_access::{
    FieldSlotMut, MapSlotMutAccess, MapSlotMutAccess as MapSlotAccessMut, SlotCustomMutAccess,
    SlotDataMutAccess, SlotDataMutAccess as SlotDataAccessMut, SlotEnumDefaultVariant,
    SlotEnumMutAccess, SlotEnumMutAccess as SlotEnumAccessMut, SlotMapValueMutAccess,
    SlotMapValueMutAccess as SlotMapValueAccessMut, SlotMutAccess, SlotMutAccess as SlotAccessMut,
    SlotMutationError, SlotOptionMutAccess, SlotOptionMutAccess as SlotOptionAccessMut,
    SlotRecordMutAccess, SlotRecordMutAccess as SlotRecordAccessMut, SlotValueMutAccess,
    SlotValueMutAccess as SlotValueMut,
};
pub use slot_mutation::{
    insert_slot_map_entry_default, remove_slot_map_entry, set_slot_option_none,
    set_slot_option_some_default, set_slot_value, set_slot_variant_default, slot_data_revision,
};
pub use slot_name::{SlotName, SlotNameError};
pub use slot_owner::SlotOwner;
pub use slot_path::{SlotPath, SlotPathError, SlotPathSegment};
pub use slot_persistence::SlotPersistence;
pub use slot_policy::SlotPolicy;
pub use slot_reader::{SlotFieldReader, SlotOptionReader, SlotReadContext};
pub use slot_record_shape::SlotRecordShape;
pub use slot_ref::SlotRef;
pub use slot_semantics::SlotSemantics;
pub use slot_shape::{
    SlotEnumEncoding, SlotFieldShape, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeIdError,
    SlotVariantShape,
};
pub use slot_shape_lookup::SlotShapeLookup;
pub use slot_value::{
    FromLpValue, OrderedF32, SlotEnumOption, SlotValue, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError,
};
pub mod shape {
    pub use super::slot_shape_builder::{
        custom, enum_external, enum_tagged, enum_with_encoding, field, field_with_policy,
        field_with_semantics, field_with_semantics_and_policy, id, leaf, map, option, record,
        reference, unit, value, variant,
    };
}
pub use slot_shape_registry::{
    SlotShapeEntry, SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot,
};
pub use slot_shape_view::{
    SlotFieldShapeView, SlotShapeView, SlotValueShapeView, SlotVariantShapeView,
};
pub use static_slot_shape::{
    StaticLpType, StaticModelEnumVariant, StaticModelStructMember, StaticSlotEnumEncoding,
    StaticSlotEnumOption, StaticSlotFieldShape, StaticSlotMeta, StaticSlotShapeDescriptor,
    StaticSlotValueShape, StaticSlotVariantShape, StaticValueEditorHint,
};

pub use crate::slots::{
    Affine2d, Affine2dSlot, ArtifactPath, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue,
    ControlProductSlot, Dim2u, Dim2uSlot, PositiveF32, PositiveF32Slot, Ratio, RatioSlot,
    RelativeNodeRefSlot, RenderOrder, RenderOrderSlot, ResourceRefSlot, SourceFileBacking,
    SourceFileSlot, SourcePath, SourcePathSlot, VisualProductSlot, Xy, XySlot,
};
pub use value_ref::ValueRef;
pub use value_slot::{MapSlot, MapSlotKeyLike, OptionSlot, SlotMapValueAccess, ValueSlot};
