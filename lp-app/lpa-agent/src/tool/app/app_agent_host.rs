//! [`AppAgentHost`]: the seam between the app agent and Studio.
//!
//! Studio implements it over its own ops, so everything the agent writes
//! lands where a user's edit lands (D3/D6). Tests and evals use stubs.

/// Injected by the embedding app.
pub trait AppAgentHost {
    /// What the user sees right now, compact: the per-turn state the
    /// session sends in the `<app_state>` block.
    fn readout(&mut self) -> String;
}
