//! Core runtime owner: [`Engine`] drives frame state, tree, bindings, and resolver.

mod content_stamp;
mod control_geometry_stamps;
mod engine;
mod engine_error;
mod engine_services;
mod entry_residency;
pub mod error;
mod frame_num;
mod frame_time;
mod loaded_project_runtime;
pub mod memory_pressure;
mod power_service;
// All three tests in this module exercise a Fixture node fed by a Shader
// node's output slot, so the module needs both node kinds.
#[cfg(all(test, feature = "node-fixture", feature = "node-shader"))]
mod output_flush_tests;
// The published-frame read proves itself on a real shader → fixture → output
// chain: the fixture is what owns the display layout, the shader is what a
// re-render would have pulled.
#[cfg(all(test, feature = "node-fixture", feature = "node-shader"))]
mod output_frame_probe_tests;
mod probe_read_backs;
// The GPU-resident latent-readback path on the render-product probe: a
// backend whose products stay GPU-resident (`supports_read_back() ==
// false`) is served the previous probe's frame one probe late.
#[cfg(all(test, feature = "node-shader"))]
mod probe_read_back_tests;
mod project_apply;
mod project_fault;
// The never-black policy's oracle: what faults a project, what must not,
// and how the verdict clears.
mod preview_sample_encoding;
#[cfg(test)]
mod project_fault_tests;
mod project_loader;
mod project_read_nodes;
mod project_read_probes;
mod project_read_resources;
mod project_read_runtime;
mod project_read_shapes;
mod project_read_stream;
mod project_runtime_index;
mod residency_applied;
#[cfg(test)]
mod resolution_persistence_tests;
// Compute-shader nodes reading a clock's timebase through `bus:time`.
#[cfg(test)]
mod scoped_resolution_tests;
#[cfg(test)]
pub(crate) mod steady_frame_alloc_tests;
// Visual shader nodes baking palette strips off a clock's timebase.
#[cfg(all(test, feature = "node-clock", feature = "node-shader"))]
mod shader_palette_tests;
#[cfg(all(test, feature = "node-clock", feature = "node-shader"))]
mod shader_timebase_tests;
mod state_root_stamps;
mod state_root_values_hash;
#[cfg(test)]
pub(crate) mod test_support;
// Every project here drives a clock through an output → fixture → shader →
// `bus:time` demand chain, so the module needs the clock, fixture and shader
// node kinds.
#[cfg(all(
    test,
    feature = "node-clock",
    feature = "node-fixture",
    feature = "node-shader"
))]
mod timebase_tests;
// A real shader compiled by a render probe mid-read, on `projects/test/basic`.
#[cfg(all(test, feature = "node-shader"))]
mod tree_entry_delta_tests;
mod tree_entry_stamps;

pub use engine::Engine;
// Consumed by fixture node tests directly and by `output_flush_tests`; both
// require the fixture-fed-by-shader combination.
#[cfg(all(test, feature = "node-fixture", feature = "node-shader"))]
pub(crate) use engine::default_demand_input_path;
pub use engine_error::EngineError;
pub use engine_services::{ButtonService, EngineServices, OutputFlushError, RadioService};
pub use frame_num::FrameNum;
pub use frame_time::FrameTime;
pub use loaded_project_runtime::LoadedProjectRuntime;
pub use power_service::{PowerError, PowerOffRequest, PowerService, PowerWakeLevel};
pub use project_apply::RuntimeApplyResult;
pub use project_fault::{FaultPresentation, ProjectFault};
pub use project_loader::{ProjectLoadError, ProjectLoader};
pub use project_read_stream::{EngineProjectReadSource, ProjectReadEventStreamError};
pub use project_runtime_index::ProjectRuntimeIndex;
pub use residency_applied::{EntryResidencyEvent, ResidencyApplied};

#[cfg(test)]
pub(crate) use engine::resolve_with_engine_host;
