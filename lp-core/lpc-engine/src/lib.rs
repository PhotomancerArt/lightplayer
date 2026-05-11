//! LightPlayer rendering engine.
//!
//! This crate provides the core rendering engine that executes shaders and manages
//! the node graph. It handles:
//! - Project loading and runtime management
//! - Node execution (shaders, textures, fixtures, outputs)
//! - Frame rendering and timing
//! - Output channel management

#![no_std]

extern crate alloc;

pub mod artifact;
pub mod binding;
pub mod bus;
pub mod control_product;
pub mod engine;
pub mod error;
pub mod gfx;
pub mod memory;
pub mod node;
pub mod nodes;
pub mod output;
pub mod project_runtime;
pub mod resolver;
pub mod runtime;
pub mod runtime_buffer;
pub mod visual_product;
pub mod wire_bridge;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactState, ArtifactStore,
};
pub use binding::{
    BindingDraft, BindingEntry, BindingError, BindingPriority, BindingRef, BindingSet,
    BindingSource, BindingTarget,
};
pub use bus::{Bus, BusError, ChannelEntry};
pub use control_product::{
    ControlExtent, ControlHint, ControlLayout, ControlProduct, ControlRenderRequest,
    ControlRenderTarget, ControlSampleFormat, ControlSpan,
};
pub use engine::{Engine, EngineError};
pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use node::{
    ControlNode, ControlRenderContext, DestroyCtx, MemPressureCtx, NodeEntry, NodeEntryState,
    NodeError, NodeRuntime, NodeTree, PressureLevel, TickContext, TreeError, tree_deltas_since,
};
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project_runtime::{
    CoreProjectLoadError, CoreProjectLoader, CoreProjectRuntime, OutputFlushError, RuntimeServices,
};
pub use resolver::{
    EngineSession, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveSession, ResolveTrace, ResolveTraceError, ResolveTraceEvent, Resolver, ResolverCache,
    SessionHostResolver, SessionResolveError, TickResolver, TraceGuard,
};
pub use runtime_buffer::{
    RuntimeBuffer, RuntimeBufferError, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeBufferStore, RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};
pub use visual_product::{
    RenderTextureRequest, TextureRenderProduct, TextureRenderProductError, VisualProduct,
    VisualSample, VisualSampleBatch, VisualSampleBatchResult, VisualSamplePoint,
};
pub use wire_bridge::{
    LpsValueToModelConversionError, lps_value_f32_to_model_value, model_type_to_lps_type,
};
