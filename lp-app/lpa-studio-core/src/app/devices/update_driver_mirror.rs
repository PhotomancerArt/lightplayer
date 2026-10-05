//! The update driver's words in the device model's mirror: its stage, its
//! end, and the person's intent the other way round. One exhaustive match
//! each, so a new driver stage or stop reason is a compile error here, never
//! a silent default on the card.

use lpa_devices::{UpdateIntentFacts, UpdateOutcomeFacts, UpdateStageFacts};
use lpa_update::{Decision, Finish, Stage, StopReason, UpdateIntent};

/// The model's stage for a driver stage.
pub(crate) fn stage_facts(stage: Stage) -> UpdateStageFacts {
    match stage {
        Stage::BackingUp => UpdateStageFacts::BackingUp,
        Stage::Updating => UpdateStageFacts::Updating,
        Stage::Restoring => UpdateStageFacts::Restoring,
        Stage::Finishing => UpdateStageFacts::Finishing,
    }
}

/// The model's outcome for how the driver ended.
pub(crate) fn outcome_facts(finish: &Finish) -> UpdateOutcomeFacts {
    match finish {
        Finish::UpToDate => UpdateOutcomeFacts::UpToDate,
        Finish::Stopped(reason) => stop_facts(reason),
    }
}

/// The model's outcome for a stop reason.
pub(crate) fn stop_facts(reason: &StopReason) -> UpdateOutcomeFacts {
    match reason {
        StopReason::Decision(decision) => decision_facts(decision),
        StopReason::Refused(_) => UpdateOutcomeFacts::Refused,
        StopReason::BoardLacksMessage(_) => UpdateOutcomeFacts::BoardLacksMessage,
        StopReason::NoCredentials => UpdateOutcomeFacts::NoCredentials,
        StopReason::NeedsEngineLogin => UpdateOutcomeFacts::NeedsEngineLogin,
        StopReason::LoginRefused => UpdateOutcomeFacts::LoginRefused,
        StopReason::MissingEngine { offline } => {
            UpdateOutcomeFacts::MissingEngine { offline: *offline }
        }
        StopReason::BackupFailed => UpdateOutcomeFacts::BackupFailed,
        StopReason::TooManyRetries => UpdateOutcomeFacts::TooManyRetries,
    }
}

/// The model's outcome for a decision the driver stops on (or the host
/// stops on before a driver exists).
pub(crate) fn decision_facts(decision: &Decision) -> UpdateOutcomeFacts {
    match decision {
        // Nothing to do, and the update's goal already reached: a heal whose
        // board runs again, or a build the board already holds.
        Decision::Nothing => UpdateOutcomeFacts::UpToDate,
        Decision::OtherTarget { .. } => UpdateOutcomeFacts::OtherTarget,
        Decision::NeedsUsb { .. } => UpdateOutcomeFacts::NeedsUsb,
        Decision::BoardIsNewer => UpdateOutcomeFacts::BoardIsNewer,
        Decision::Busy { .. } => UpdateOutcomeFacts::Busy,
        Decision::NoUpdateForPlayOnly => UpdateOutcomeFacts::PlayOnly,
        Decision::RefusedBuild { .. } => UpdateOutcomeFacts::RefusedBuild,
        Decision::ReportCrashing { .. } => UpdateOutcomeFacts::Crashing,
        // The driver never stops on these: it acts on them. Reaching here
        // means a host stopped before acting, which only a heal-only run
        // does when it cannot act (no engine), and that reports as such.
        Decision::Heal { .. }
        | Decision::Reinstall { .. }
        | Decision::ContinueUpdate { .. }
        | Decision::OfferUpdate { .. } => UpdateOutcomeFacts::MissingEngine { offline: false },
    }
}

/// The driver's intent for the model's.
pub(crate) fn driver_intent(intent: &UpdateIntentFacts) -> UpdateIntent {
    match intent {
        UpdateIntentFacts::Auto => UpdateIntent::Auto,
        UpdateIntentFacts::Install {
            allow_downgrade, ..
        } => UpdateIntent::Install {
            allow_downgrade: *allow_downgrade,
        },
        UpdateIntentFacts::Reinstall => UpdateIntent::Reinstall,
    }
}

#[cfg(test)]
mod tests {
    use lpa_update::HostRefusal;

    use super::*;

    #[test]
    fn every_stop_lands_on_its_own_outcome() {
        let cases = [
            (Finish::UpToDate, UpdateOutcomeFacts::UpToDate),
            (
                Finish::Stopped(StopReason::NoCredentials),
                UpdateOutcomeFacts::NoCredentials,
            ),
            (
                Finish::Stopped(StopReason::MissingEngine { offline: true }),
                UpdateOutcomeFacts::MissingEngine { offline: true },
            ),
            (
                Finish::Stopped(StopReason::Decision(Decision::ReportCrashing {
                    build_id: "x".into(),
                })),
                UpdateOutcomeFacts::Crashing,
            ),
            (
                Finish::Stopped(StopReason::Decision(Decision::NoUpdateForPlayOnly)),
                UpdateOutcomeFacts::PlayOnly,
            ),
            (
                Finish::Stopped(StopReason::Refused(HostRefusal::NeedsLogin)),
                UpdateOutcomeFacts::Refused,
            ),
        ];
        for (finish, want) in cases {
            assert_eq!(outcome_facts(&finish), want, "{finish:?}");
        }
    }

    #[test]
    fn intents_and_stages_map_one_to_one() {
        assert_eq!(
            driver_intent(&UpdateIntentFacts::Install {
                version: "2026.10.05-2".into(),
                allow_downgrade: true
            }),
            UpdateIntent::Install {
                allow_downgrade: true
            }
        );
        assert_eq!(
            driver_intent(&UpdateIntentFacts::Reinstall),
            UpdateIntent::Reinstall
        );
        assert_eq!(stage_facts(Stage::Finishing), UpdateStageFacts::Finishing);
    }
}
