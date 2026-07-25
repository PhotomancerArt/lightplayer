//! [`AgentFeedback`]: progress reported by a spawned agent run.
//!
//! The run executes as a spawned task while the actor owns the controller,
//! so every event rides the command queue back in (like console commands:
//! applied synchronously, in order, never coalesced).

use lpa_agent::AgentEvent;

use crate::app::agent::agent_session_key::AgentSessionKey;

/// One message from a running agent session to the controller.
#[derive(Clone, Debug)]
pub enum AgentFeedback {
    /// A streamed session event (text delta, tool progress, turn/usage).
    Event {
        key: AgentSessionKey,
        event: AgentEvent,
    },
    /// The run future finished; the session runtime is back in its slot.
    /// `error` carries the provider failure message when the run failed.
    RunEnded {
        key: AgentSessionKey,
        error: Option<String>,
    },
}
