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

pub mod access_gate;
pub mod access_guarded_fs;
pub mod access_state;
pub mod access_store;
pub mod device_identity;
pub mod error;
pub mod file_sync;
pub mod handlers;
pub mod heartbeat_status;
pub mod link_session;
pub mod panel_state;
#[cfg(feature = "node-power-button")]
mod power_off;
pub mod project;
pub mod project_manager;
mod project_read_source;
pub mod recovery_report;
pub mod server;

pub use access_gate::{Required, classify};
pub use access_guarded_fs::AccessGuardedFs;
pub use access_state::EntropySource;
pub use device_identity::{DEVICE_IDENTITY_PATH, read_device_uid};
pub use error::ServerError;
pub use heartbeat_status::HeartbeatStatus;
pub use link_session::LinkSession;
pub use lpc_engine::products::visual::{
    ConsumerPolicy, RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSpace,
};
pub use lpc_engine::{
    ButtonService, LpGraphics, LpShader, PowerError, PowerOffRequest, PowerService, PowerWakeLevel,
    RadioService, ShaderCompileOptions, ShaderFrontend,
};
// Manifest-core inputs, re-exported so embedders that reach lpc-engine only
// through this crate can assemble their firmware manifest (M2).
pub use lpc_engine::features::{ENGINE_FEATURE_FRAGMENT, supported_features};
#[cfg(feature = "node-power-button")]
pub use power_off::{PowerOffQueue, PowerPlatform};
pub use project::Project;
pub use project_manager::{ProjectManager, is_project_dir};
pub use server::{
    LpServer, MemoryStatsFn, PROJECT_LOAD_MIN_HEADROOM_BYTES, PROJECT_READ_MIN_HEADROOM_BYTES,
    ReadHeadroomProbe, RebootHook,
};

/// GLSL frontend that ships on LightPlayer devices — the product constant.
///
/// Device hosts (`fw-esp32c6`, `fw-emu`, and device-emulating hosts such as
/// `fw-host` and `lp-cli`) pass this when constructing their CPU graphics
/// backend. It is stated exactly once, here: frontend selection is an
/// explicit host decision, never a Cargo-feature default, so feature
/// unification can no longer flip which frontend a build compiles with.
pub const DEVICE_SHADER_FRONTEND: ShaderFrontend = ShaderFrontend::LpsGlsl;
