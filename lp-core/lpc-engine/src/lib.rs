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

pub mod dataflow;
pub mod engine;
pub mod node;
pub mod nodes;
pub mod product;
pub mod products;
pub mod resource;
pub mod resources;
pub mod shader_abi;

pub use engine::error::Error;
pub use engine::{
    ButtonService, Engine, EngineError, EngineProjectReadSource, EngineServices, FrameNum,
    FrameTime, OutputFlushError, ProjectLoadError, ProjectLoader, ProjectReadEventStreamError,
    RadioService, RuntimeApplyResult,
};
// Graphics seam re-exports: the traits/handles live in `lp-gfx`; the
// cfg-selected CPU implementation is `lp_gfx_lpvm::LpvmGraphics` (constructed
// by hosts, injected via `Engine::set_graphics`). `ShaderFrontend` is the
// host's explicit GLSL-frontend product decision, passed when constructing
// the backend.
pub use lp_gfx::{GfxError, LpGraphics, LpShader, ShaderCompileOptions};
pub use lp_shader::ShaderFrontend;
