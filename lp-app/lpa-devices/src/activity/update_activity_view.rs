//! A running Update activity, as typed facts for the card's words.

use serde::{Deserialize, Serialize};

use super::update_intent_facts::UpdateIntentFacts;
use super::update_outcome_facts::UpdateOutcomeFacts;
use super::update_stage_facts::UpdateStageFacts;

/// What an Update activity is doing, so the app layer can word the card
/// without string-matching a label. Data only — the card's buttons are
/// offers, never fields here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpdateActivityView {
    /// What the update was asked to do (the version it installs, if one).
    pub intent: UpdateIntentFacts,
    /// The last stage the driver reported; `None` before the first.
    pub stage: Option<UpdateStageFacts>,
    /// The stage's progress: bytes moved and the piece's length.
    pub done: u32,
    pub total: u32,
    /// How the driver said it ended, once it has (just before the activity
    /// does).
    pub outcome: Option<UpdateOutcomeFacts>,
    /// The board reset (or its link dropped) and the activity is waiting for
    /// it to come back: the card keeps the stage and percent and says it is
    /// reconnecting — never "Offline".
    pub between_legs: bool,
}
