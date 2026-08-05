//! What happens to the setup flow. Design: `docs/design/device-setup-flow.md` §2.
//!
//! Two sources, one vocabulary: user gestures (CTAs, board tiles, retry,
//! ✕) and outcomes the executor reports back (port granted, probe done,
//! flash succeeded/failed, project generated, push complete). The reducer
//! cannot tell them apart and does not need to.

use super::verdict::BoardProbe;

#[derive(Debug, Clone, PartialEq)]
pub enum SetupEvent {
    /// CONNECT_INTRO's primary CTA: "It's connected".
    ItsConnected,
    /// CONNECT_INTRO's secondary CTA, and PROBE_FAILED's board link.
    PickBoardFirst,
    /// BOARD_FIRST's "it's plugged in" after the guidance was read.
    ItsPluggedIn,
    /// Step back toward CONNECT_INTRO (BOARD_FIRST back, BOARD_PICK back,
    /// WLED_FOUND cancel, PROBE_FAILED back).
    Back,
    /// A board tile was picked (BOARD_FIRST or BOARD_PICK).
    BoardChosen {
        board_id: String,
    },
    /// The step's forward verb: BOARD_PICK's "Flash" (or, on a target that
    /// needs no flash, "Continue"), and PROVISION's "Create".
    Confirm,
    /// The browser's port chooser returned a grant.
    PortGranted,
    /// The user dismissed the port chooser.
    PortPickerCancelled,
    /// The chooser had nothing to offer.
    PortPickerEmpty,
    /// One probe pass finished.
    ProbeCompleted {
        probe: BoardProbe,
    },
    /// The port went away.
    PortLost,
    /// WLED_FOUND's "Wipe and set up".
    WipeAndSetUp,
    /// ALREADY_LP's "Done" — adopt.
    AdoptDone,
    /// ALREADY_LP's "Set it up fresh…".
    SetUpFresh,
    /// PROBE_FAILED's retry, and FLASH_FAILED's.
    Retry,
    FlashSucceeded,
    FlashFailed {
        detail: String,
    },
    /// The provision step's name field was edited.
    NameEdited {
        name: String,
    },
    /// The generator installed the package.
    ProjectGenerated {
        project_uid: String,
    },
    /// The push landed.
    PushCompleted,
    /// ✕.
    CloseRequested,
    /// ABANDON_GUARD's "Keep flashing".
    KeepFlashing,
    /// ABANDON_GUARD's and FLASH_FAILED's "Abandon".
    Abandon,
}

impl SetupEvent {
    pub fn kind(&self) -> SetupEventKind {
        match self {
            Self::ItsConnected => SetupEventKind::ItsConnected,
            Self::PickBoardFirst => SetupEventKind::PickBoardFirst,
            Self::ItsPluggedIn => SetupEventKind::ItsPluggedIn,
            Self::Back => SetupEventKind::Back,
            Self::BoardChosen { .. } => SetupEventKind::BoardChosen,
            Self::Confirm => SetupEventKind::Confirm,
            Self::PortGranted => SetupEventKind::PortGranted,
            Self::PortPickerCancelled => SetupEventKind::PortPickerCancelled,
            Self::PortPickerEmpty => SetupEventKind::PortPickerEmpty,
            Self::ProbeCompleted { .. } => SetupEventKind::ProbeCompleted,
            Self::PortLost => SetupEventKind::PortLost,
            Self::WipeAndSetUp => SetupEventKind::WipeAndSetUp,
            Self::AdoptDone => SetupEventKind::AdoptDone,
            Self::SetUpFresh => SetupEventKind::SetUpFresh,
            Self::Retry => SetupEventKind::Retry,
            Self::FlashSucceeded => SetupEventKind::FlashSucceeded,
            Self::FlashFailed { .. } => SetupEventKind::FlashFailed,
            Self::NameEdited { .. } => SetupEventKind::NameEdited,
            Self::ProjectGenerated { .. } => SetupEventKind::ProjectGenerated,
            Self::PushCompleted => SetupEventKind::PushCompleted,
            Self::CloseRequested => SetupEventKind::CloseRequested,
            Self::KeepFlashing => SetupEventKind::KeepFlashing,
            Self::Abandon => SetupEventKind::Abandon,
        }
    }
}

/// An event's identity without its data — the other axis of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupEventKind {
    ItsConnected,
    PickBoardFirst,
    ItsPluggedIn,
    Back,
    BoardChosen,
    Confirm,
    PortGranted,
    PortPickerCancelled,
    PortPickerEmpty,
    ProbeCompleted,
    PortLost,
    WipeAndSetUp,
    AdoptDone,
    SetUpFresh,
    Retry,
    FlashSucceeded,
    FlashFailed,
    NameEdited,
    ProjectGenerated,
    PushCompleted,
    CloseRequested,
    KeepFlashing,
    Abandon,
}

impl SetupEventKind {
    /// Every event, for the exhaustive transition table. Kept honest the
    /// same way [`SetupStateKind::ALL`](super::SetupStateKind::ALL) is.
    pub const ALL: [Self; 23] = [
        Self::ItsConnected,
        Self::PickBoardFirst,
        Self::ItsPluggedIn,
        Self::Back,
        Self::BoardChosen,
        Self::Confirm,
        Self::PortGranted,
        Self::PortPickerCancelled,
        Self::PortPickerEmpty,
        Self::ProbeCompleted,
        Self::PortLost,
        Self::WipeAndSetUp,
        Self::AdoptDone,
        Self::SetUpFresh,
        Self::Retry,
        Self::FlashSucceeded,
        Self::FlashFailed,
        Self::NameEdited,
        Self::ProjectGenerated,
        Self::PushCompleted,
        Self::CloseRequested,
        Self::KeepFlashing,
        Self::Abandon,
    ];

    pub fn ordinal(self) -> usize {
        match self {
            Self::ItsConnected => 0,
            Self::PickBoardFirst => 1,
            Self::ItsPluggedIn => 2,
            Self::Back => 3,
            Self::BoardChosen => 4,
            Self::Confirm => 5,
            Self::PortGranted => 6,
            Self::PortPickerCancelled => 7,
            Self::PortPickerEmpty => 8,
            Self::ProbeCompleted => 9,
            Self::PortLost => 10,
            Self::WipeAndSetUp => 11,
            Self::AdoptDone => 12,
            Self::SetUpFresh => 13,
            Self::Retry => 14,
            Self::FlashSucceeded => 15,
            Self::FlashFailed => 16,
            Self::NameEdited => 17,
            Self::ProjectGenerated => 18,
            Self::PushCompleted => 19,
            Self::CloseRequested => 20,
            Self::KeepFlashing => 21,
            Self::Abandon => 22,
        }
    }

    /// Stable label for event-log records and test failure messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::ItsConnected => "its-connected",
            Self::PickBoardFirst => "pick-board-first",
            Self::ItsPluggedIn => "its-plugged-in",
            Self::Back => "back",
            Self::BoardChosen => "board-chosen",
            Self::Confirm => "confirm",
            Self::PortGranted => "port-granted",
            Self::PortPickerCancelled => "port-picker-cancelled",
            Self::PortPickerEmpty => "port-picker-empty",
            Self::ProbeCompleted => "probe-completed",
            Self::PortLost => "port-lost",
            Self::WipeAndSetUp => "wipe-and-set-up",
            Self::AdoptDone => "adopt-done",
            Self::SetUpFresh => "set-up-fresh",
            Self::Retry => "retry",
            Self::FlashSucceeded => "flash-succeeded",
            Self::FlashFailed => "flash-failed",
            Self::NameEdited => "name-edited",
            Self::ProjectGenerated => "project-generated",
            Self::PushCompleted => "push-completed",
            Self::CloseRequested => "close-requested",
            Self::KeepFlashing => "keep-flashing",
            Self::Abandon => "abandon",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_kind_is_listed_exactly_once() {
        let mut seen: Vec<usize> = SetupEventKind::ALL.iter().map(|k| k.ordinal()).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..SetupEventKind::ALL.len()).collect::<Vec<_>>());
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<&str> = SetupEventKind::ALL.iter().map(|k| k.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), SetupEventKind::ALL.len());
    }
}
