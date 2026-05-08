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
pub mod engine;
pub mod error;
pub mod gfx;
pub mod node;
pub mod nodes;
pub mod output;
pub mod panic_node;
pub mod project_runtime;
pub mod prop;
pub mod render_product;
pub mod resolver;
pub mod runtime;
pub mod runtime_buffer;
pub mod runtime_product;
pub mod wire_bridge;
pub mod memory;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactManager, ArtifactState,
};
pub use binding::{
    BindingDraft, BindingEntry, BindingError, BindingId, BindingPriority, BindingRegistry,
    BindingSource, BindingTarget,
};
pub use bus::{Bus, BusError, ChannelEntry};
pub use engine::{Engine, EngineError};
pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use node::{
    DestroyCtx, NodeEntryState, MemPressureCtx, NodeRuntime, NodeEntry, NodeError, NodeTree, PressureLevel,
    TickContext, TreeError, tree_deltas_since,
};
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project_runtime::{
    CoreProjectLoadError, CoreProjectLoader, CoreProjectRuntime, LoadedNodeDef, OutputFlushError,
    RuntimeServices, SourceAuthoringIndex,
};
pub use prop::{ProducedSlotAccess, RuntimeStateAccess};
pub use render_product::{
    RenderProduct, RenderProductError, RenderProductId, RenderProductStore, RenderSample,
    RenderSampleBatch, RenderSampleBatchResult, RenderSamplePoint, TextureRenderProduct,
    TextureRenderProductError,
};
pub use resolver::{
    Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel, ResolveSession,
    ResolveTrace, ResolveTraceError, ResolveTraceEvent, Resolver, ResolverCache,
    SessionHostResolver, SessionResolveError, TickResolver, TraceGuard,
};
pub use runtime_buffer::{
    RuntimeBuffer, RuntimeBufferError, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeBufferStore, RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};
pub use runtime_product::{RuntimeProduct, RuntimeProductError};
pub use wire_bridge::{
    LpsValueToModelConversionError, lps_value_f32_to_model_value, model_type_to_lps_type,
};
