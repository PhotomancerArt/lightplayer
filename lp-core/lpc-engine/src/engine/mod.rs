//! Core runtime owner: [`Engine`] drives frame state, tree, bindings, and resolver.

mod engine;
mod engine_error;
mod engine_services;
pub mod error;
mod frame_num;
mod frame_time;
mod loaded_project_runtime;
pub mod memory_pressure;
#[cfg(test)]
mod output_flush_tests;
mod project_apply;
mod project_loader;
mod project_read;
mod project_read_nodes;
mod project_read_probes;
mod project_read_resources;
mod project_read_runtime;
mod project_read_shapes;
mod project_read_stream;
mod project_runtime_index;
#[cfg(test)]
pub(crate) mod test_support;

pub use engine::Engine;
#[cfg(test)]
pub(crate) use engine::default_demand_input_path;
pub use engine_error::EngineError;
pub use engine_services::{ButtonService, EngineServices, OutputFlushError, RadioService};
pub use frame_num::FrameNum;
pub use frame_time::FrameTime;
pub use loaded_project_runtime::LoadedProjectRuntime;
pub use project_apply::RuntimeApplyResult;
pub use project_loader::{ProjectLoadError, ProjectLoader};
pub use project_read_stream::EngineProjectReadSource;
pub use project_runtime_index::ProjectRuntimeIndex;

#[cfg(test)]
pub(crate) use engine::resolve_with_engine_host;
