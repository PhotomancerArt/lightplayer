//! LightPlayer **core model** crate: **foundation** types for identity,
//! addressing, portable values, and slot-shaped data. Wire/protocol shapes live
//! in `lpc-wire`.
//!
//! Authored node definitions (Project / Texture / Shader / Output / Fixture)
//! live in `lpc-source`.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "derive")]
pub use lpc_slot_macros::SlotRecord;

#[doc(hidden)]
pub mod __private {
    pub use alloc::vec::Vec;
}

// --- Foundation -------------------------------------------------------------------------------

pub mod error;
pub mod node;
pub mod prop;
pub mod slot;
pub mod types;

// --- Shared surface (non-wire) ---------------------------------------------------------------

pub mod bus;
pub mod lp_config;
pub mod lp_path;
pub mod serial;
pub mod tree;

pub mod project;
pub mod resource;
mod versioned;
// --- Foundation re-exports ------------------------------------------------------------------

pub use prop::constraint;
pub use prop::kind;

pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
/// Cross-cutting error for domain property access and validation.
pub use error::DomainError;
/// Legacy semantic value kind used by the pre-slot property model.
///
/// New slot-model code should prefer typed slot leaf descriptors whose semantic
/// meaning owns its storage shape.
pub use kind::Kind;
pub use prop::Versioned;
pub use prop::{ModelStructMember, ModelType, ModelValue};

pub use lp_config::LightplayerConfig;
pub use lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use node::node_prop_spec::NodePropSpec;
pub use node::{
    NodeId, NodeName, NodeNameError, RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc,
};
pub use project::{FrameId, ProjectConfig};
pub use project::{advance_state_version, current_state_version, set_current_state_version};
pub use prop::value_path::ValuePath;
pub use resource::{RenderProductId, ResourceDomain, ResourceRef, RuntimeBufferId};
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use slot::{
    Affine2d, Affine2dSlot, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    FromModelValue, OrderedF32, PositiveF32Slot, RatioSlot, RelativeNodeRefSlot, RenderOrderSlot,
    ResourceRefSlot, SlotEditorHint, SlotEnumOption, SlotLeaf, SlotLeafError, SlotLeafId,
    SlotMapValueAccess, SlotValueShape, SourcePathSlot, ToModelValue, XySlot, affine2d_shape,
    artifact_path_shape, color_order_shape, dim2u_shape, positive_f32_shape, ratio_shape,
    relative_node_ref_shape, render_order_shape, render_product_resource_shape, resource_ref_shape,
    runtime_buffer_resource_shape, source_path_shape, xy_shape,
};
pub use slot::{
    SlotAccess, SlotData, SlotDataAccess, SlotDataKind, SlotEnum, SlotEnumAccess, SlotEnumShape,
    SlotFieldShape, SlotMap, SlotMapAccess, SlotMapDyn, SlotMapKey, SlotMapKeyLike,
    SlotMapKeyShape, SlotMeta, SlotName, SlotNameError, SlotOption, SlotOptionAccess,
    SlotOptionDyn, SlotOwner, SlotPath, SlotPathError, SlotRecord, SlotRecordAccess,
    SlotRecordShape, SlotRef, SlotShape, SlotShapeId, SlotShapeIdError, SlotShapeKind,
    SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot, SlotTree,
    SlotValidationError, SlotValue, SlotValueAccess, SlotVariantShape, StaticSlotAccess, ValueRef,
    VersionedSlotShape,
};
pub use tree::tree_path::{NodePathSegment, PathError, TreePath};
