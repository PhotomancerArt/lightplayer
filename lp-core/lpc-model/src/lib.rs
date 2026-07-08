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

pub use lpc_slot_macros::{SlotValue, Slotted};

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
pub mod slot_sync_codec;
pub mod value;

// --- Shared surface (non-wire) ---------------------------------------------------------------

pub mod bus;
pub mod config;
pub mod control;
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
pub mod slot_views {
    include!(concat!(env!("OUT_DIR"), "/slot_views.rs"));
}

pub mod artifact;
pub mod hardware_endpoint_spec;
pub mod nodes;
pub mod product;
pub mod products;
pub mod project;
pub mod resource;
pub mod resources;
pub mod server;
pub mod slots;
pub mod sync;

#[cfg(feature = "schema-gen")]
mod schema_gen_smoke;

#[cfg(feature = "schema-gen")]
pub mod schema_gen;
// --- Foundation re-exports ------------------------------------------------------------------

pub use value::constraint;
pub use value::kind;

pub use artifact::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactLocationError, ArtifactReadRoot, ArtifactSpec,
    SrcArtifactLibRef,
};
pub use binding::{
    BindingDef, BindingDefError, BindingDefView, BindingDefs, BindingRef, BindingRefError,
    BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError,
};
pub use bus::ChannelName;
pub use constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
/// Legacy semantic value kind used by the pre-slot property model.
///
/// New slot-model code should prefer typed slot leaf descriptors whose semantic
/// meaning owns its storage shape.
pub use kind::Kind;
pub use project::inventory::{
    AssetBodyOrigin, AssetChange, AssetChangeKind, AssetChangeSummary, AssetContentType,
    AssetEntry, AssetLocation, AssetState, NodeUseChange, NodeUseChangeKind, NodeUseChangeSummary,
    ReferencedAsset,
};
pub use value::WithRevision;
pub use value::{LpType, LpValue, ModelEnumVariant, ModelStructMember};

pub use config::DEFAULT_SERIAL_BAUD_RATE;
pub use control::{CONTROL_MESSAGE_SHAPE_NAME, ControlMessage, TriggerEvent};
pub use hardware_endpoint_spec::{HardwareEndpointSpecError, HwEndpointSpec};
pub use lpfs::fs_event::FsVersion;
pub use lpfs::lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use node::node_prop_spec::NodePropSpec;
pub use node::tree_path::{NodePathSegment, PathError, TreePath};
pub use node::{
    NodeArtifact, NodeDef, NodeDefChange, NodeDefChangeKind, NodeDefChangeSummary, NodeDefEntry,
    NodeDefLocation, NodeDefState, NodeDefValidationError, NodeId, NodeInvocation,
    NodeInvocationSlot, NodeKind, NodeName, NodeNameError, NodeRuntimeStatus, RelativeNodeRef,
    RelativeNodeRefError, RelativeNodeRefSrc,
};
pub use nodes::{
    AddSubMode, ArtifactPathResolutionError, ButtonDef, ButtonDefView, ButtonState,
    ButtonStateView, ClockControls, ClockDef, ClockDefView, ClockState, ColorOrder,
    ComputeShaderDef, ComputeShaderDefView, ControlRadioDef, ControlRadioDefView,
    ControlRadioState, ControlRadioStateView, DivMode, FixtureDef, FixtureDefView,
    FixtureDiagnosticMode, FixtureSamplingConfig, FixtureState, FixtureStateView, FluidDef,
    FluidDefView, FluidEmitter, FluidState, GlslOpts, GlslOptsView, InvocationSite, MappingConfig,
    MulMode, NodeDefParseError, OutputDef, OutputDefView, OutputDriverOptionsConfig,
    OutputDriverOptionsConfigView, PROJECT_FORMAT_VERSION, PathSpec, PlaylistDef, PlaylistDefView,
    PlaylistEntry, PlaylistEntryView, PlaylistState, PlaylistStateView, ProjectDef, ProjectDefView,
    ProjectFormatProbe, RingOrder, ScalarHint, ScalarHintView, ShaderDef, ShaderDefView,
    ShaderHeaderGenError, ShaderMapKeyDef, ShaderParamDef, ShaderParamDefView, ShaderSlotDef,
    ShaderSlotKind, ShaderSlotMappingDef, ShaderSlotMappingKind, ShaderState, ShaderStateView,
    ShaderValueShapeRef, TextureDef, TextureDefView, TextureFormat, TextureState, TextureStateView,
    generate_compute_shader_header, read_project_format_json, resolve_artifact_specifier,
};
pub use product::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlProduct,
    ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, ProductKind, ProductRef,
    VisualProduct,
};
pub use project::overlay::{
    ArtifactOverlay, AssetBodyOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
pub use project::overlay_mutation::{
    MutationCmd, MutationCmdBatch, MutationCmdBatchResult, MutationCmdId, MutationCmdResult,
    MutationCmdStatus, MutationEffect, MutationOp, MutationRejection, MutationRejectionReason,
    StoredSlotEdit,
};
pub use project::{
    ChangeSummary, CommitResult, LocationSeg, MutationBatchResults, MutationResult,
    NodeUseLocation, ProjectChangeSummary, ProjectConfig, ProjectInventory, ProjectNode,
    ProjectNodeOrigin, ProjectNodePlacement, ProjectTree, Revision,
};
pub use project::{advance_revision, current_revision, set_current_revision};
pub use resource::{ResourceDomain, ResourceRef, RuntimeBufferId, runtime_buffer_resource_shape};
pub use server::server_config::ServerConfig;
pub use slot::{
    Affine2d, Affine2dSlot, ArtifactPath, ArtifactPathSlot, AssetSlot, AssetSlotValue,
    ColorOrderSlot, ColorOrderValue, ControlProductSlot, Dim2u, Dim2uSlot, FromLpValue, OrderedF32,
    PositiveF32, PositiveF32Slot, Ratio, RatioSlot, RelativeNodeRefSlot, RenderOrder,
    RenderOrderSlot, ResourceRefSlot, SlotEnumOption, SlotMapValueAccess, SlotValue,
    SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError, VisualProductSlot, Xy, XySlot,
};
pub use slot::{
    DynamicSlotObject, EnumSlot, FieldSlot, FieldSlotMut, MapSlot, MapSlotAccess, MapSlotAccessMut,
    MapSlotKeyLike, MapSlotMutAccess, OptionSlot, SlotAccess, SlotAccessMut, SlotAccessor,
    SlotAccessorError, SlotAccessorStep, SlotCustomAccess, SlotCustomMutAccess, SlotData,
    SlotDataAccess, SlotDataAccessMut, SlotDataMutAccess, SlotDirection, SlotEnum, SlotEnumAccess,
    SlotEnumAccessMut, SlotEnumDefaultVariant, SlotEnumEncoding, SlotEnumMutAccess, SlotEnumShape,
    SlotFactory, SlotFactoryError, SlotFactoryFn, SlotFieldReader, SlotFieldShape, SlotLookupError,
    SlotMapDyn, SlotMapKey, SlotMapKeyShape, SlotMapValueAccessMut, SlotMapValueMutAccess,
    SlotMerge, SlotMeta, SlotMutAccess, SlotMutationError, SlotName, SlotNameError,
    SlotOptionAccess, SlotOptionAccessMut, SlotOptionDyn, SlotOptionMutAccess, SlotOptionReader,
    SlotOwner, SlotPath, SlotPathError, SlotPathSegment, SlotPolicy, SlotPolicyResolution,
    SlotReadContext, SlotRecord, SlotRecordAccess, SlotRecordAccessMut, SlotRecordMutAccess,
    SlotRecordShape, SlotRef, SlotSemantics, SlotShape, SlotShapeEntry, SlotShapeId,
    SlotShapeIdError, SlotShapeLookup, SlotShapeRegistry, SlotShapeRegistryError,
    SlotShapeRegistrySnapshot, SlotShapeView, SlotValueAccess, SlotValueMut, SlotValueMutAccess,
    SlotValueShapeView, SlotVariantShape, SlotVariantShapeView, SlottedEnum, SlottedEnumMut,
    StaticLpType, StaticModelEnumVariant, StaticModelStructMember, StaticSlotAccess,
    StaticSlotEnumEncoding, StaticSlotEnumOption, StaticSlotFieldShape, StaticSlotMeta,
    StaticSlotShape, StaticSlotShapeDescriptor, StaticSlotValueShape, StaticSlotVariantShape,
    StaticValueEditorHint, ValueRef, ValueSlot, create_dynamic_slot_data, ensure_slot_present,
    insert_slot_map_entry_default, lookup_slot_data, lookup_slot_data_and_shape,
    lookup_slot_data_mut, lp_value_matches_type, remove_slot_map_entry, resolve_slot_policy,
    resolve_slot_policy_and_leaf, set_slot_option_none, set_slot_option_some_default,
    set_slot_value, set_slot_variant_default, slot_data_revision,
};
pub use value::value_path::ValuePath;
