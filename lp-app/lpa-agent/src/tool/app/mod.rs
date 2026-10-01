//! The app agent's toolset: the tools that build and edit a whole project
//! and act on the app (`edit_project`, `read`, `act` arrive in P03, P06,
//! P08), over the [`AppAgentHost`] seam Studio implements.

pub mod app_agent_host;
pub mod app_toolset;

pub use app_agent_host::AppAgentHost;
pub use app_toolset::AppToolset;
