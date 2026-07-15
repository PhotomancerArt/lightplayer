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

pub mod device_identity;
pub mod error;
pub mod file_sync;
pub mod handlers;
pub mod project;
pub mod project_manager;
mod project_read_source;
pub mod recovery_report;
pub mod server;
pub mod template;

pub use device_identity::{DEVICE_IDENTITY_PATH, read_device_uid};
pub use error::ServerError;
pub use lpc_engine::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
pub use lpc_engine::{ButtonService, LpGraphics, LpShader, RadioService, ShaderCompileOptions};
pub use project::Project;
pub use project_manager::ProjectManager;
pub use server::{LpServer, MemoryStatsFn};
