//! The wizard's *gestures*: the subset of [`SetupEvent`] a person performs.
//!
//! Design: `docs/design/device-setup-flow.md` §2. The reducer cannot tell a
//! user gesture from an outcome the executor reports, and does not need to
//! — but the DISPATCH path does. Ops travel as [`HomeOp`](crate::HomeOp)
//! values, which are `Eq` (the action layer compares them); [`SetupEvent`]
//! is not, because `ProbeCompleted` carries a registry row with an `f64`
//! timestamp. Rather than weaken the op vocabulary, the gestures — which
//! carry nothing but strings — are their own enum.
//!
//! The second reason is a boundary worth keeping: `ProbeCompleted`,
//! `FlashSucceeded`, `PortGranted` and friends are things the WORLD says.
//! A component cannot fabricate them, because it cannot name them.

use super::event::SetupEvent;

/// One gesture on the setup wizard card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupGesture {
    /// CONNECT_INTRO's primary CTA.
    ItsConnected,
    /// CONNECT_INTRO's secondary CTA, and PROBE_FAILED's board link.
    PickBoardFirst,
    /// BOARD_FIRST's "it's plugged in".
    ItsPluggedIn,
    /// Step back toward CONNECT_INTRO.
    Back,
    /// A board tile was picked.
    BoardChosen { board_id: String },
    /// The step's forward verb (Flash / Continue / Create).
    Confirm,
    /// WLED_FOUND's "Wipe and set up".
    WipeAndSetUp,
    /// ALREADY_LP's "Done".
    AdoptDone,
    /// ALREADY_LP's "Set it up fresh…".
    SetUpFresh,
    /// PROBE_FAILED's and FLASH_FAILED's retry.
    Retry,
    /// The provision step's name field.
    NameEdited { name: String },
    /// ✕.
    CloseRequested,
    /// ABANDON_GUARD's "Keep flashing".
    KeepFlashing,
    /// ABANDON_GUARD's and FLASH_FAILED's "Abandon".
    Abandon,
}

impl From<SetupGesture> for SetupEvent {
    fn from(gesture: SetupGesture) -> Self {
        match gesture {
            SetupGesture::ItsConnected => Self::ItsConnected,
            SetupGesture::PickBoardFirst => Self::PickBoardFirst,
            SetupGesture::ItsPluggedIn => Self::ItsPluggedIn,
            SetupGesture::Back => Self::Back,
            SetupGesture::BoardChosen { board_id } => Self::BoardChosen { board_id },
            SetupGesture::Confirm => Self::Confirm,
            SetupGesture::WipeAndSetUp => Self::WipeAndSetUp,
            SetupGesture::AdoptDone => Self::AdoptDone,
            SetupGesture::SetUpFresh => Self::SetUpFresh,
            SetupGesture::Retry => Self::Retry,
            SetupGesture::NameEdited { name } => Self::NameEdited { name },
            SetupGesture::CloseRequested => Self::CloseRequested,
            SetupGesture::KeepFlashing => Self::KeepFlashing,
            SetupGesture::Abandon => Self::Abandon,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::setup_flow::event::SetupEventKind;

    /// The events NO gesture may produce: things the world reports.
    const OUTCOMES: [SetupEventKind; 9] = [
        SetupEventKind::PortGranted,
        SetupEventKind::PortPickerCancelled,
        SetupEventKind::PortPickerEmpty,
        SetupEventKind::ProbeCompleted,
        SetupEventKind::PortLost,
        SetupEventKind::FlashSucceeded,
        SetupEventKind::FlashFailed,
        SetupEventKind::ProjectGenerated,
        SetupEventKind::PushCompleted,
    ];

    fn every_gesture() -> Vec<SetupGesture> {
        vec![
            SetupGesture::ItsConnected,
            SetupGesture::PickBoardFirst,
            SetupGesture::ItsPluggedIn,
            SetupGesture::Back,
            SetupGesture::BoardChosen {
                board_id: "seeed/xiao-esp32-c6".to_string(),
            },
            SetupGesture::Confirm,
            SetupGesture::WipeAndSetUp,
            SetupGesture::AdoptDone,
            SetupGesture::SetUpFresh,
            SetupGesture::Retry,
            SetupGesture::NameEdited {
                name: "Porch sign".to_string(),
            },
            SetupGesture::CloseRequested,
            SetupGesture::KeepFlashing,
            SetupGesture::Abandon,
        ]
    }

    #[test]
    fn every_gesture_names_a_distinct_event() {
        let mut kinds: Vec<_> = every_gesture()
            .into_iter()
            .map(|gesture| SetupEvent::from(gesture).kind())
            .collect();
        let count = kinds.len();
        kinds.sort_by_key(|kind| kind.ordinal());
        kinds.dedup();
        assert_eq!(kinds.len(), count, "two gestures collapsed onto one event");
    }

    #[test]
    fn the_gestures_and_the_outcomes_partition_the_event_vocabulary() {
        // Nothing the UI can press is an outcome, and nothing is missing:
        // a new event has to be classified as one or the other before this
        // passes again.
        let mut covered: Vec<usize> = every_gesture()
            .into_iter()
            .map(|gesture| SetupEvent::from(gesture).kind().ordinal())
            .chain(OUTCOMES.iter().map(|kind| kind.ordinal()))
            .collect();
        covered.sort_unstable();
        assert_eq!(
            covered,
            (0..SetupEventKind::ALL.len()).collect::<Vec<_>>(),
            "every event is either a gesture or an outcome, exactly once"
        );
    }
}
