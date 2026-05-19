//! Core runtime owner: [`Engine`] drives frame state, tree, bindings, resolver, and artifacts.

mod engine;
mod engine_error;
mod engine_services;
pub mod error;
mod frame_num;
mod frame_time;
pub mod memory_pressure;
#[cfg(test)]
mod output_flush_tests;
mod project_loader;
mod project_read;
mod project_read_nodes;
mod project_read_probes;
mod project_read_resources;
mod project_read_runtime;
mod project_read_shapes;
mod project_read_stream;
mod slot_mutation;
#[cfg(test)]
pub(crate) mod test_support;

pub use engine::Engine;
#[cfg(test)]
pub(crate) use engine::default_demand_input_path;
pub use engine_error::EngineError;
pub use engine_services::{EngineServices, OutputFlushError};
pub use frame_num::FrameNum;
pub use frame_time::FrameTime;
pub use project_loader::{ProjectLoadError, ProjectLoader};

#[cfg(test)]
pub(crate) use engine::resolve_with_engine_host;
