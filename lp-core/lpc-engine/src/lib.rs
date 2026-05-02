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
pub mod legacy;
pub mod legacy_project;
pub mod node;
pub mod nodes;
pub mod output;
pub mod panic_node;
pub mod prop;
pub mod render_product;
pub mod resolver;
pub mod runtime;
pub mod runtime_buffer;
pub mod runtime_product;
pub mod tree;
pub mod wire_bridge;

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
pub use legacy::nodes::{FixtureRuntime, OutputRuntime, ShaderRuntime, TextureRuntime};
pub use legacy::output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use legacy_project::{LegacyProjectRuntime, MemoryStatsFn};
pub use node::{DestroyCtx, MemPressureCtx, Node, NodeError, PressureLevel, TickContext};
pub use nodes::LegacyNodeRuntime;
pub use prop::RuntimePropAccess;
pub use render_product::{
    RenderProduct, RenderProductError, RenderProductId, RenderProductStore, RenderSample,
    RenderSampleBatch, RenderSampleBatchResult, RenderSamplePoint,
};
pub use resolver::{
    BindingKind, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveSession, ResolveSource, ResolveTrace, ResolveTraceError, ResolveTraceEvent,
    ResolvedSlot, Resolver, ResolverCache, SessionHostResolver, SessionResolveError,
    SlotResolverCache, TickResolver, TraceGuard,
};
pub use runtime::{NodeInitContext, RenderContext};
pub use runtime_buffer::{
    RuntimeBuffer, RuntimeBufferError, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeBufferStore, RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};
pub use runtime_product::{RuntimeProduct, RuntimeProductError};
pub use tree::{EntryState, NodeEntry, NodeTree, TreeError, tree_deltas_since};
pub use wire_bridge::{
    LpsValueToModelConversionError, lps_value_f32_to_model_value, model_type_to_lps_type,
};
