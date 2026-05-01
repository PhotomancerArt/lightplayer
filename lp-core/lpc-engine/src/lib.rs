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

pub mod bus;
pub mod error;
pub mod gfx;
pub mod nodes;
pub mod output;
pub mod panic_node;
pub mod project;
pub mod prop;
pub mod resolver;
pub mod runtime;
pub mod tree;
pub mod wire_bridge;

pub use bus::{Bus, BusError, ChannelEntry};
pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use nodes::LegacyNodeRuntime;
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project::{MemoryStatsFn, ProjectRuntime};
pub use prop::RuntimePropAccess;
pub use resolver::{BindingKind, ResolveSource, ResolvedSlot, ResolverCache};
pub use runtime::{NodeInitContext, RenderContext};
pub use tree::{EntryState, NodeEntry, NodeTree, TreeError, tree_deltas_since};
pub use wire_bridge::{lps_value_f32_to_model_value, model_type_to_lps_type};
