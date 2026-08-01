//! [`ToolPhase`]: what one `iterate` call is doing right now.
//!
//! Emitted through the progress callback injected into
//! [`run_iterate`](crate::tool::iterate_tool::run_iterate) (and surfaced to
//! the UI as [`AgentEvent::ToolProgress`](crate::session::agent_event::
//! AgentEvent::ToolProgress)) so the running tool row shows live activity
//! instead of a generic spinner.

use core::fmt;

/// One phase of an `iterate` execution, in emission order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPhase {
    /// New source is being staged as an overlay edit.
    Staging,
    /// The base shader (and health report) is compiling/evaluating.
    Compiling,
    /// Probe `i` of `of` is evaluating (1-based).
    Probing { i: u32, of: u32 },
    /// Waiting (bounded) for the live engine's post-stage verdict.
    AwaitingEngine,
    /// Serializing the result.
    Finishing,
}

impl fmt::Display for ToolPhase {
    /// The short UI label the running tool row renders.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Staging => write!(f, "staging edit"),
            Self::Compiling => write!(f, "compiling"),
            Self::Probing { i, of } => write!(f, "probe {i}/{of}"),
            Self::AwaitingEngine => write!(f, "waiting for engine"),
            Self::Finishing => write!(f, "finishing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_compact_and_probe_is_indexed() {
        assert_eq!(ToolPhase::Staging.to_string(), "staging edit");
        assert_eq!(ToolPhase::Probing { i: 2, of: 5 }.to_string(), "probe 2/5");
        assert_eq!(ToolPhase::AwaitingEngine.to_string(), "waiting for engine");
    }
}
