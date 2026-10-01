//! [`AppAgentHost`]: the seam between the app agent and Studio.
//!
//! Studio implements it over its own ops, so everything the agent writes
//! lands where a user's edit lands (D3/D6). Tests and evals use stubs.

use crate::tool::app::edit_project_tool::{EditProjectInput, ProjectEditsOutcome};
use crate::tool::app::read_tool::ReadInput;
use crate::tool::iterate_host::{HostError, HostFuture};

/// Injected by the embedding app.
pub trait AppAgentHost {
    /// What the user sees right now, compact: the per-turn state the
    /// session sends in the `<app_state>` block.
    fn readout(&mut self) -> String;

    /// Apply one `edit_project` call's edits, in order, through the
    /// user's own ops; one status per edit. `Err` only when the host could
    /// not run the batch at all (no project open, no answer in time).
    fn apply_project_edits<'a>(
        &'a mut self,
        input: &'a EditProjectInput,
    ) -> HostFuture<'a, Result<ProjectEditsOutcome, HostError>> {
        let _ = input;
        Box::pin(async { Err(HostError::new("this host cannot edit projects")) })
    }

    /// Read one thing in full (the `read` tool). `Err` = not found, with
    /// what does exist.
    fn read<'a>(
        &'a mut self,
        input: &'a ReadInput,
    ) -> HostFuture<'a, Result<serde_json::Value, HostError>> {
        let _ = input;
        Box::pin(async { Err(HostError::new("this host cannot read")) })
    }
}
