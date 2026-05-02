//! Core project runtime: [`CoreProjectRuntime`] owns the engine and service surface
//! for the M4 MVP path.

mod compatibility_projection;
mod core_project_runtime;
mod project_loader;
mod runtime_services;

pub use compatibility_projection::CompatibilityProjection;
pub use core_project_runtime::CoreProjectRuntime;
pub use project_loader::{CoreProjectLoadError, CoreProjectLoader};
pub use runtime_services::{OutputFlushError, RuntimeServices};
