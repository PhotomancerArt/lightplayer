//! Slot identity and value-reference model.
//!
//! A slot is a named location owned by a node or bus. A [`ValuePath`] navigates
//! inside the value exposed at that slot; it is not part of the slot identity.

mod slot_access;
mod slot_data;
mod slot_enum_shape;
mod slot_meta;
mod slot_name;
mod slot_owner;
mod slot_path;
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
pub use slot_data::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};
pub use slot_enum_shape::SlotEnumShape;
pub use slot_meta::SlotMeta;
pub use slot_name::{SlotName, SlotNameError};
pub use slot_owner::SlotOwner;
pub use slot_path::{SlotPath, SlotPathError, SlotPathSegment};
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
    affine2d_shape, artifact_path_shape, color_order_shape, dim2u_shape, positive_f32_shape, ratio_shape, relative_node_ref_shape,
    render_order_shape, render_product_resource_shape, resource_ref_shape, runtime_buffer_resource_shape, source_path_shape,
    u32_list_shape, xy_shape, Affine2d, Affine2dSlot, ArtifactPathSlot, ColorOrderSlot,
    ColorOrderValue, Dim2u, Dim2uSlot, PositiveF32Slot,
    RatioSlot, RelativeNodeRefSlot, RenderOrderSlot,
    ResourceRefSlot, SourcePathSlot, XySlot,
};
pub use value_ref::ValueRef;
pub use value_slot::{MapSlot, MapSlotKeyLike, OptionSlot, SlotMapValueAccess, ValueSlot};
