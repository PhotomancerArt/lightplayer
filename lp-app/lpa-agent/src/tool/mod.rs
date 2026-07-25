//! The `iterate` tool and the host seam it dispatches through.

pub mod iterate_host;
pub mod iterate_tool;

pub use iterate_host::{AgentHost, BindingInfo, FixtureSummary, HostError, ShaderContext};
pub use iterate_tool::{
    ITERATE_TOOL_NAME, IterateInput, IterateOutcome, MAX_SOURCE_BYTES, iterate_tool_def,
    run_iterate,
};
