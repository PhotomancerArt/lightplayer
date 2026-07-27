//! Project-scoped command envelopes.

mod node_command;
mod project_command;

pub use node_command::{WireNodeCommand, WireNodeCommandResponse};
pub use project_command::{WireProjectCommand, WireProjectCommandResponse};
