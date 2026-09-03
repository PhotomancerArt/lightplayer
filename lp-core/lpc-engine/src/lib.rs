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
#[cfg(test)]
extern crate std;

/// The test binary counts allocations so a steady frame's churn is a unit
/// test (`engine::steady_frame_alloc_tests`), not an emulator run.
#[cfg(test)]
pub(crate) mod test_alloc_counter;
#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: test_alloc_counter::CountingAlloc = test_alloc_counter::CountingAlloc;

pub mod color;
pub mod dataflow;
pub mod engine;
pub mod features;
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
pub use features::supported_features;
// Graphics seam re-exports: the traits/handles live in `lp-gfx`; the
// cfg-selected CPU implementation is `lp_gfx_lpvm::LpvmGraphics` (constructed
// by hosts, injected via `Engine::set_graphics`). `ShaderFrontend` is the
// host's explicit GLSL-frontend product decision, passed when constructing
// the backend.
pub use lp_gfx::{GfxError, LpGraphics, LpShader, ShaderCompileOptions};
pub use lp_shader::ShaderFrontend;
