//! Slot identity and value-reference model.
//!
//! A slot is a named location owned by a node or bus. A [`ValuePath`] navigates
//! inside the value exposed at that slot; it is not part of the slot identity.

mod slot_access;
mod slot_data;
mod slot_leaf;
mod slot_meta;
mod slot_name;
mod slot_owner;
mod slot_path;
mod slot_ref;
mod slot_registry;
mod slot_shape;
mod slot_shape_registry;
mod slot_tree;
mod slot_value;
mod value_ref;

pub use slot_access::{
    SlotAccess, SlotDataAccess, SlotEnumAccess, SlotMapAccess, SlotOptionAccess, SlotRecordAccess,
    SlotValueAccess, StaticSlotAccess,
};
pub use slot_data::{SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotOptionDyn, SlotRecord};
pub use slot_leaf::{
    Affine2d, Affine2dSlot, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    FromModelValue, OrderedF32, PositiveF32Slot, RatioSlot, RelativeNodeRefSlot, RenderOrderSlot,
    SlotEditorHint, SlotEnumOption, SlotLeaf, SlotLeafError, SlotLeafId, SlotValueShape,
    SourcePathSlot, ToModelValue, XySlot, affine2d_shape, artifact_path_shape, color_order_shape,
    dim2u_shape, positive_f32_shape, ratio_shape, relative_node_ref_shape, render_order_shape,
    source_path_shape, xy_shape,
};
pub use slot_meta::SlotMeta;
pub use slot_name::{SlotName, SlotNameError};
pub use slot_owner::SlotOwner;
pub use slot_path::{SlotPath, SlotPathError};
pub use slot_ref::SlotRef;
pub use slot_registry::{SlotRegistry, SlotRegistryError};
pub use slot_shape::{
    SlotFieldShape, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeIdError, SlotVariantShape,
};
pub use slot_shape_registry::{
    SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot, VersionedSlotShape,
};
pub use slot_tree::{SlotDataKind, SlotShapeKind, SlotTree, SlotValidationError};
pub use slot_value::{SlotMap, SlotMapKeyLike, SlotMapValueAccess, SlotOption, SlotValue};
pub use value_ref::ValueRef;
