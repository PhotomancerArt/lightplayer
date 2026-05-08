//! Core project runtime: [`CoreProjectRuntime`] owns the engine and service surface
//! for the M4 MVP path.

mod core_project_runtime;
mod project_loader;
#[allow(
    dead_code,
    reason = "Retained for M3 canonical project sync resource projection"
)]
mod resource_projection;
mod runtime_services;
mod source_authoring_index;

pub use core_project_runtime::CoreProjectRuntime;
pub use project_loader::{CoreProjectLoadError, CoreProjectLoader, LoadedNodeDef};
pub use runtime_services::{OutputFlushError, RuntimeServices};
pub use source_authoring_index::SourceAuthoringIndex;
