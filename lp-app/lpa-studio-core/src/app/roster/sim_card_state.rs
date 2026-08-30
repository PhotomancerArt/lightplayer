//! The live simulator card's status vocabulary.
//!
//! What is left of the roster vocabulary after the device-system teardown
//! (M2 of the device-model rebuild). The 19-variant `RosterCardState` was
//! the device card's state machine AND, incidentally, the sim card's — the
//! sim only ever derived two of its rows. Those two rows are this enum;
//! the device vocabulary is rebuilt from the new model's projection DTOs.
//!
//! The sim has no link, no boot, no readiness and no registry entry (D22),
//! so the session's EXISTENCE is its status: it is either running a
//! project or it is empty.

use crate::UiStatusKind;

/// Where the live sim card stands. The session exists (its card exists
/// only while it does), so the only question is whether a project is on it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SimCardState {
    /// Running the project load-as-push put on it.
    Running,
    /// Live, nothing loaded — an empty simulator is fine.
    #[default]
    Empty,
}

impl SimCardState {
    /// The card's status line (health only — never project names).
    pub fn status_line(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Empty => "Connected — nothing loaded",
        }
    }

    /// The status tone the card's tint edge and section rollups read.
    /// Both sim states are healthy: a running sim and an empty one are
    /// each exactly what they claim to be.
    pub fn tone(self) -> UiStatusKind {
        UiStatusKind::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_states_are_healthy_and_say_what_they_are() {
        assert_eq!(SimCardState::Running.status_line(), "Running");
        assert_eq!(
            SimCardState::Empty.status_line(),
            "Connected — nothing loaded"
        );
        assert_eq!(SimCardState::Running.tone(), UiStatusKind::Good);
        assert_eq!(SimCardState::Empty.tone(), UiStatusKind::Good);
    }
}
