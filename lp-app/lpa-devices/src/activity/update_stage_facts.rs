//! Where an update is — the model's mirror of `lpa_update::Stage`, plus the
//! one stage the driver does not name itself.

use serde::{Deserialize, Serialize};

/// The stage an [`ActivityMarker::UpdateStage`](crate::ActivityMarker::UpdateStage)
/// reports.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UpdateStageFacts {
    /// Reading the board's current engine back, so it can be put back.
    /// Nothing on the board has changed yet — the one stage Cancel is
    /// offered in.
    BackingUp,
    /// Moving the new core.
    Updating,
    /// Putting the board's own engine back (a heal).
    Restoring,
    /// The new core fetching its engine.
    Finishing,
    /// Another device holds the board's transfer; this Studio asks again
    /// and takes over when it is free. Not a driver stage: the effects layer
    /// reports it off the driver's `Busy` decision.
    Waiting,
}

impl UpdateStageFacts {
    /// The model's own plain label for the stage (the card's words are the
    /// app layer's, which replaces these).
    pub fn label(self) -> &'static str {
        match self {
            Self::BackingUp => "Backing up current firmware…",
            Self::Updating => "Updating firmware…",
            Self::Restoring => "Restoring firmware…",
            Self::Finishing => "Finishing the update…",
            Self::Waiting => "Another device is updating it…",
        }
    }

    /// Whether a cancel can still leave the board untouched: only while
    /// backing up — once writing starts there is no Cancel.
    pub fn allows_cancel(self) -> bool {
        matches!(self, Self::BackingUp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_backing_up_allows_a_cancel() {
        assert!(UpdateStageFacts::BackingUp.allows_cancel());
        for stage in [
            UpdateStageFacts::Updating,
            UpdateStageFacts::Restoring,
            UpdateStageFacts::Finishing,
            UpdateStageFacts::Waiting,
        ] {
            assert!(!stage.allows_cancel(), "{stage:?}");
            assert!(!stage.label().is_empty());
        }
    }
}
