//! LightPlayer server implementation.
//!
//! This crate provides the server-side implementation that manages projects,
//! handles client requests, and coordinates with the rendering engine.
//! It includes:
//! - Project management and lifecycle
//! - Request handling and routing
//! - File system operations
//! - Server initialization and configuration

#![no_std]

pub mod error;
pub mod file_sync;
pub mod handlers;
pub mod project;
pub mod project_manager;
mod project_read_source;
pub mod recovery_report;
pub mod server;
pub mod template;

pub use error::ServerError;
pub use lpc_engine::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
pub use lpc_engine::{
    ButtonService, LpGraphics, LpShader, RadioService, ShaderCompileOptions, ShaderFrontend,
};
pub use project::Project;
pub use project_manager::ProjectManager;
pub use server::{LpServer, MemoryStatsFn};

/// GLSL frontend that ships on LightPlayer devices — the product constant.
///
/// Device hosts (`fw-esp32`, `fw-emu`, and device-emulating hosts such as
/// `fw-host` and `lp-cli`) pass this when constructing their CPU graphics
/// backend. It is stated exactly once, here: frontend selection is an
/// explicit host decision, never a Cargo-feature default, so feature
/// unification can no longer flip which frontend a build compiles with.
pub const DEVICE_SHADER_FRONTEND: ShaderFrontend = ShaderFrontend::LpsGlsl;
