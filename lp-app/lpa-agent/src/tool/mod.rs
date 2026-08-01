//! The `iterate` + `upsert_param` tools and the host seam they dispatch
//! through.

pub mod iterate_host;
pub mod iterate_tool;
pub mod params_section;
pub mod tool_phase;
pub mod upsert_param_tool;

pub use iterate_host::{
    AgentHost, BindingInfo, EngineStatusKind, EngineVerdict, FixtureSummary, HostError, HostFuture,
    ParamDefRecord, ParamUpsert, ShaderContext,
};
pub use iterate_tool::{
    ENGINE_VERDICT_BUDGET_MS, ITERATE_TOOL_NAME, IterateInput, IterateOutcome, MAX_SOURCE_BYTES,
    iterate_tool_def, run_iterate,
};
pub use tool_phase::ToolPhase;
pub use upsert_param_tool::{
    UPSERT_PARAM_TOOL_NAME, UpsertParamInput, run_upsert_param, upsert_param_tool_def,
};
