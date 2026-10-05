//! The app agent's toolset: the tools that build and edit a whole project
//! and act on the app (`edit_project`, `read`, `act` arrive in P03, P06,
//! P08), over the [`AppAgentHost`] seam Studio implements.

pub mod act_tool;
pub mod app_agent_host;
pub mod app_tool_schema;
pub mod app_toolset;
pub mod edit_project_tool;
pub mod read_tool;

pub use act_tool::{ACT_TOOL_NAME, ActArgValue, ActInput, ActOutcome, act_tool_def, run_act};
pub use app_agent_host::AppAgentHost;
pub use app_toolset::AppToolset;
pub use edit_project_tool::{
    CreateNodeEdit, EDIT_PROJECT_TOOL_NAME, EditProjectInput, EditStatus, ImportPatternEdit,
    NodeRef, ProjectEdit, ProjectEditsOutcome, SetAssetEdit, SetEdit, SetTargetEdit, SlotRef,
    edit_project_tool_def, run_edit_project,
};
pub use read_tool::{READ_TOOL_NAME, ReadInput, ReadWhat, read_tool_def, run_read};
