//! The `iterate` tool and the host seam it dispatches through.

pub mod iterate_host;
pub mod iterate_tool;
pub mod tool_phase;

pub use iterate_host::{
    AgentHost, BindingInfo, EngineStatusKind, EngineVerdict, FixtureSummary, HostError, HostFuture,
    ShaderContext,
};
pub use iterate_tool::{
    ENGINE_VERDICT_BUDGET_MS, ITERATE_TOOL_NAME, IterateInput, IterateOutcome, MAX_SOURCE_BYTES,
    iterate_tool_def, run_iterate,
};
pub use tool_phase::ToolPhase;
