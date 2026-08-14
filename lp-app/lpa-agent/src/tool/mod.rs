//! The `iterate` + `upsert_param` + `declare_space` tools and the host
//! seam they dispatch through.

pub mod declare_space_tool;
pub mod iterate_host;
pub mod iterate_tool;
pub mod params_section;
pub mod tool_phase;
pub mod upsert_param_tool;

pub use declare_space_tool::{
    DECLARE_SPACE_TOOL_NAME, DeclareSpaceInput, declare_space_tool_def, entry_point,
    run_declare_space,
};
pub use iterate_host::{
    AgentHost, BindingInfo, DeclaredSpace, EngineStatusKind, EngineVerdict, FixtureSummary,
    HostError, HostFuture, ParamDefRecord, ParamUpsert, ProjectionShapeTag, ShaderContext,
    SpaceDeclaration,
};
pub use iterate_tool::{
    ENGINE_VERDICT_BUDGET_MS, ITERATE_TOOL_NAME, IterateInput, IterateOutcome, MAX_SOURCE_BYTES,
    iterate_tool_def, run_iterate,
};
pub use tool_phase::ToolPhase;
pub use upsert_param_tool::{
    UPSERT_PARAM_TOOL_NAME, UpsertParamInput, run_upsert_param, upsert_param_tool_def,
};
