//! The toolset seam: what an [`AgentSession`](crate::AgentSession) offers
//! the model and how it runs a call. One loop, two toolsets — the shader
//! agent's (`iterate` + `upsert_param` + `declare_space`) and the app
//! agent's ([`crate::tool::app`]).

pub mod shader_toolset;
pub mod tool_outcome;
pub mod toolset;

pub use shader_toolset::ShaderToolset;
pub use tool_outcome::ToolOutcome;
pub use toolset::{APP_STATE_CLOSE, APP_STATE_OPEN, Toolset, wrap_turn_state};
