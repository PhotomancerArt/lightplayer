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
pub mod memory;
pub mod node;
pub mod nodes;
pub mod product;
pub mod products;
pub mod resolver;
pub mod resource;
pub mod resources;

pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactState, ArtifactStore,
};
pub use binding::{
    BindingDraft, BindingEntry, BindingError, BindingPriority, BindingRef, BindingSet,
    BindingSource, BindingTarget,
};
pub use bus::{Bus, BusError, ChannelEntry};
pub use engine::{
    Engine, EngineError, EngineServices, FrameNum, FrameTime, OutputFlushError, ProjectLoadError,
    ProjectLoader,
};
pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use gfx::{
    LpsValueToModelConversionError, lps_value_f32_to_model_value, model_type_to_lps_type,
};
pub use node::{
    ControlNode, ControlRenderContext, DestroyCtx, MemPressureCtx, NodeEntry, NodeEntryState,
    NodeError, NodeRuntime, NodeTree, PressureLevel, TickContext, TreeError, tree_deltas_since,
};
pub use product::{
    ControlExtent, ControlHint, ControlLayout, ControlProduct, ControlRenderRequest,
    ControlRenderTarget, ControlSampleFormat, ControlSpan, RenderTextureRequest,
    TextureRenderProduct, TextureRenderProductError, VisualProduct, VisualSample,
    VisualSampleBatch, VisualSampleBatchResult, VisualSamplePoint,
};
pub use resolver::{
    EngineSession, Production, ProductionSource, QueryKey, ResolveHost, ResolveLogLevel,
    ResolveSession, ResolveTrace, ResolveTraceError, ResolveTraceEvent, Resolver, ResolverCache,
    SessionHostResolver, SessionResolveError, TickResolver, TraceGuard,
};
pub use resource::{
    RuntimeBuffer, RuntimeBufferError, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeBufferStore, RuntimeChannelSampleFormat, RuntimeColorLayout, RuntimeTextureFormat,
};
