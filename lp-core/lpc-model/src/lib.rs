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

pub use lpc_slot_macros::{SlotRecord, SlotValue};

#[doc(hidden)]
pub mod __private {
    pub use alloc::boxed::Box;
    pub use alloc::string::String;
    pub use alloc::vec::Vec;
}

// --- Foundation -------------------------------------------------------------------------------

pub mod binding;
pub mod node;
pub mod slot;
pub mod slot_codec;
pub mod value;

// --- Shared surface (non-wire) ---------------------------------------------------------------

pub mod bus;
pub mod config;
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
pub mod slot_views {
    include!(concat!(env!("OUT_DIR"), "/slot_views.rs"));
}

pub mod artifact;
pub mod nodes;
pub mod product;
pub mod products;
pub mod project;
pub mod resource;
pub mod resources;
pub mod server;
pub mod slots;
pub mod sync;
// --- Foundation re-exports ------------------------------------------------------------------

pub use value::constraint;
pub use value::kind;

pub use artifact::{ArtifactLocator, SrcArtifactLibRef};
pub use binding::{
    BindingDef, BindingDefError, BindingDefView, BindingDefs, BindingEndpoint,
    BindingEndpointError, BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError,
};
pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
/// Legacy semantic value kind used by the pre-slot property model.
///
/// New slot-model code should prefer typed slot leaf descriptors whose semantic
/// meaning owns its storage shape.
pub use kind::Kind;
pub use value::WithRevision;
pub use value::{LpType, LpValue, ModelEnumVariant, ModelStructMember};

pub use config::DEFAULT_SERIAL_BAUD_RATE;
pub use lpfs::lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use node::node_prop_spec::NodePropSpec;
pub use node::tree_path::{NodePathSegment, PathError, TreePath};
pub use node::{
    NodeDef, NodeId, NodeInvocation, NodeInvocationView, NodeKind, NodeName, NodeNameError,
    RelativeNodeRef, RelativeNodeRefError, RelativeNodeRefSrc,
};
pub use nodes::{
    AddSubMode, ColorOrder, DivMode, FixtureDef, FixtureDefView, FixtureSamplingConfig,
    FixtureState, FixtureStateView, GlslOpts, GlslOptsView, MappingConfig, MulMode,
    NodeDefParseError, OutputDef, OutputDefView, OutputDriverOptionsConfig,
    OutputDriverOptionsConfigView, PathSpec, ProjectDef, ProjectDefView, RingOrder, ScalarHint,
    ScalarHintView, ShaderDef, ShaderDefView, ShaderParamDef, ShaderParamDefView, ShaderState,
    ShaderStateView, TextureDef, TextureDefView, TextureFormat, TextureState, TextureStateView,
};
pub use product::{ControlExtent, ControlProduct, ProductKind, ProductRef, VisualProduct};
pub use project::{ProjectConfig, Revision};
pub use project::{advance_revision, current_revision, set_current_revision};
pub use resource::{ResourceDomain, ResourceRef, RuntimeBufferId, runtime_buffer_resource_shape};
pub use server::server_config::ServerConfig;
pub use slot::{
    Affine2d, Affine2dSlot, ArtifactPath, ArtifactPathSlot, ColorOrderSlot, ColorOrderValue,
    ControlProductSlot, Dim2u, Dim2uSlot, FromLpValue, OrderedF32, PositiveF32, PositiveF32Slot,
    Ratio, RatioSlot, RelativeNodeRefSlot, RenderOrder, RenderOrderSlot, ResourceRefSlot,
    SlotEnumOption, SlotMapValueAccess, SlotValue, SlotValueShape, SourcePath, SourcePathSlot,
    ToLpValue, ValueEditorHint, ValueRootError, VisualProductSlot, Xy, XySlot,
};
pub use slot::{
    DynamicSlotObject, EnumSlot, FieldSlot, FieldSlotMut, MapSlot, MapSlotAccess, MapSlotKeyLike,
    MapSlotMutAccess, OptionSlot, SlotAccess, SlotAccessor, SlotAccessorError, SlotAccessorStep,
    SlotData, SlotDataAccess, SlotDataMutAccess, SlotEnum, SlotEnumAccess, SlotEnumDefaultVariant,
    SlotEnumMutAccess, SlotEnumShape, SlotFactory, SlotFactoryError, SlotFactoryFn,
    SlotFieldReader, SlotFieldShape, SlotLookupError, SlotMapDyn, SlotMapKey, SlotMapKeyShape,
    SlotMapValueMutAccess, SlotMeta, SlotMutAccess, SlotMutationError, SlotName, SlotNameError,
    SlotOptionAccess, SlotOptionDyn, SlotOptionMutAccess, SlotOptionReader, SlotOwner, SlotPath,
    SlotPathError, SlotPathSegment, SlotReadContext, SlotRecord, SlotRecordAccess,
    SlotRecordMutAccess, SlotRecordShape, SlotRef, SlotShape, SlotShapeEntry, SlotShapeId,
    SlotShapeIdError, SlotShapeRegistry, SlotShapeRegistryError, SlotShapeRegistrySnapshot,
    SlotValueAccess, SlotValueMutAccess, SlotVariantShape, SlottedEnum, SlottedEnumMut,
    StaticSlotAccess, StaticSlotShape, ValueRef, ValueSlot, create_dynamic_slot_data,
    insert_slot_map_entry_default, lookup_slot_data, set_slot_option_some_default, set_slot_value,
    set_slot_variant_default, slot_data_revision,
};
pub use value::value_path::ValuePath;
