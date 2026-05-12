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
pub mod dataflow;
pub mod engine;
pub mod gfx;
pub mod node;
pub mod nodes;
pub mod product;
pub mod products;
pub mod resource;
pub mod resources;

pub use engine::error::Error;
pub use engine::{
    Engine, EngineError, EngineServices, FrameNum, FrameTime, OutputFlushError, ProjectLoadError,
    ProjectLoader,
};
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
