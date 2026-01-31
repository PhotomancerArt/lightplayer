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

pub mod error;
pub mod nodes;
pub mod output;
pub mod project;
pub mod runtime;

pub use error::Error;
pub use nodes::{NodeConfig, NodeRuntime};
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project::ProjectRuntime;
pub use runtime::{NodeInitContext, RenderContext};
