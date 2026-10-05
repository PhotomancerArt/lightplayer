//! What the person asked an update to do — the model's mirror of
//! `lpa_update::UpdateIntent`.
//!
//! A mirror and not the type itself: `lpa-update` depends on this crate, so
//! this crate cannot depend back, and it stays serde-only either way. The
//! effects layer (`lpa-studio-core`) maps this onto the driver's intent; the
//! one field the driver's type does not carry, `version`, is how the effects
//! layer picks the BUILD the intent installs (this Studio's own, or the
//! store's — "Other version…").

use serde::{Deserialize, Serialize};

/// The intent an [`Action::Update`](crate::Action::Update) carries.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum UpdateIntentFacts {
    /// The decision table as it stands: heal, finish, or the offered update
    /// — what the controller spawns with no click (heal and finish start
    /// themselves).
    #[default]
    Auto,
    /// Put `version` on the board (Update, "Install X", "Other version…").
    /// `allow_downgrade`: the person chose a version older than the board's.
    Install {
        version: String,
        allow_downgrade: bool,
    },
    /// Write a crashing board's own engine again.
    Reinstall,
}

impl UpdateIntentFacts {
    /// Whether this is the no-click intent the controller spawns by itself.
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intents_round_trip_and_auto_is_the_default() {
        assert!(UpdateIntentFacts::default().is_auto());
        for intent in [
            UpdateIntentFacts::Auto,
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: true,
            },
            UpdateIntentFacts::Reinstall,
        ] {
            let json = serde_json::to_string(&intent).expect("serialize");
            let back: UpdateIntentFacts = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, intent);
        }
    }
}
