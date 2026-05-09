//! LightPlayer **core model** crate: **foundation** types for identity,
//! addressing, portable values, and slot-shaped data. Wire/protocol shapes live
//! in `lpc-wire`.
//!
//! Authored node definitions live here too. The slot model is the shared domain
//! language for source artifacts, wire sync, runtime inspection, and UI editing.

#![no_std]

extern crate alloc;
extern crate self as lpc_model;

#[cfg(feature = "std")]
extern crate std;

pub use lpc_slot_macros::SlotRecord;

#[doc(hidden)]
pub mod __private {
    pub use alloc::vec::Vec;
}

// --- Foundation -------------------------------------------------------------------------------

pub mod binding;
pub mod node;
pub mod slot;
pub mod value;

// --- Shared surface (non-wire) ---------------------------------------------------------------

pub mod bus;
pub mod config;
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}

pub mod artifact;
pub mod nodes;
pub mod project;
pub mod resource;
pub mod server;
pub mod slots;
pub mod sync;
// --- Foundation re-exports ------------------------------------------------------------------

pub use value::constraint;
pub use value::kind;

pub use artifact::{ArtifactLocator, SrcArtifactLibRef};
pub use binding::{
    BindingDef, BindingDefError, BindingDefs, BindingEndpoint, BindingEndpointError, BusSlotRef,
    BusSlotRefError, NodeSlotRef, NodeSlotRefError,
};
pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
/// Legacy semantic value kind used by the pre-slot property model.
///
/// New slot-model code should prefer typed slot leaf descriptors whose semantic
/// meaning owns its storage shape.
pub use kind::Kind;
pub use value::WithRevision;
pub use value::{LpType, LpValue, ModelStructMember};

pub use config::DEFAULT_SERIAL_BAUD_RATE;
pub use lpfs::lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use node::node_prop_spec::NodePropSpec;
pub use node::tree_path::{NodePathSegment, PathError, TreePath};
pub use node::{
    NodeDef, NodeId, NodeInvocation, NodeKind, NodeName, NodeNameError, RelativeNodeRef,
    RelativeNodeRefError, RelativeNodeRefSrc,
};
pub use nodes::{
    AddSubMode, ColorOrder, DivMode, FixtureDef, GlslOpts, MappingConfig, MulMode, OutputDef,
    OutputDriverOptionsConfig, PathSpec, ProjectDef, RingOrder, ScalarHint, ShaderDef,
    ShaderParamDef, ShaderState, TextureDef, TextureFormat, TextureState,
};
pub use project::{ProjectConfig, Revision};
pub use project::{advance_revision, current_revision, set_current_revision};
pub use resource::{RenderProduct, RenderProductId, ResourceDomain, ResourceRef, RuntimeBufferId};
pub use server::server_config::ServerConfig;
pub use slot::{
    Affine2d, Affine2dSlot, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    FromLpValue, OrderedF32, PositiveF32Slot, RatioSlot, RelativeNodeRefSlot, RenderOrderSlot,
    RenderProductSlot, ResourceRefSlot, SlotEnumOption, SlotMapValueAccess, SlotValue,
    SlotValueShape, SourcePathSlot, ToLpValue, ValueEditorHint, ValueRootError, XySlot,
    affine2d_shape, artifact_path_shape, color_order_shape, dim2u_shape, positive_f32_shape,
    ratio_shape, relative_node_ref_shape, render_order_shape, render_product_resource_shape,
    render_product_shape, resource_ref_shape, runtime_buffer_resource_shape, source_path_shape,
    u32_list_shape, xy_shape,
};
pub use slot::{
    FieldSlot, MapSlot, MapSlotAccess, MapSlotKeyLike, OptionSlot, SlotAccess, SlotData,
    SlotDataAccess, SlotEnum, SlotEnumAccess, SlotEnumShape, SlotFieldShape, SlotLookupError,
    SlotMapDyn, SlotMapKey, SlotMapKeyShape, SlotMeta, SlotName, SlotNameError, SlotOptionAccess,
    SlotOptionDyn, SlotOwner, SlotPath, SlotPathError, SlotPathSegment, SlotRecord,
    SlotRecordAccess, SlotRecordShape, SlotRef, SlotShape, SlotShapeEntry, SlotShapeId,
    SlotShapeIdError, SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot,
    SlotValueAccess, SlotVariantShape, StaticSlotAccess, StaticSlotShape, ValueRef, ValueSlot,
    lookup_slot_data,
};
pub use value::value_path::ValuePath;
