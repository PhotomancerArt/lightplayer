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
    /// The chooser resolved with a port, and the connect it starts is
    /// still running. Reported mid-op (the executor's one honest
    /// mid-point), which is why it asks for nothing: the work it would
    /// name is already in flight.
    PortChosen,
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
    /// STALE_LP's "Update the firmware" — the only way past a board this
    /// Studio cannot talk to.
    UpdateFirmware,
    /// ALREADY_LP's "Done" — adopt and STAY on the gallery.
    AdoptDone,
    /// ALREADY_LP's "Open in the editor →" — adopt and land in the editor
    /// lensed to the board (what "Done" used to do).
    AdoptAndOpen,
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
    /// The target was set up by a route that is not this flow, while the
    /// flow was still asking for a board: an "Open in sim" landed a
    /// project on the simulator, and that project's advisory manifest
    /// `target` is the board it implies (honest-device-preview vision D4).
    ///
    /// `board_id: None` means nothing could be inferred — the untargeted
    /// project — and the picker is still the only way to answer.
    SetUpElsewhere {
        board_id: Option<String>,
    },
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
            Self::PortChosen => SetupEventKind::PortChosen,
            Self::PortGranted => SetupEventKind::PortGranted,
            Self::PortPickerCancelled => SetupEventKind::PortPickerCancelled,
            Self::PortPickerEmpty => SetupEventKind::PortPickerEmpty,
            Self::ProbeCompleted { .. } => SetupEventKind::ProbeCompleted,
            Self::PortLost => SetupEventKind::PortLost,
            Self::WipeAndSetUp => SetupEventKind::WipeAndSetUp,
            Self::UpdateFirmware => SetupEventKind::UpdateFirmware,
            Self::AdoptDone => SetupEventKind::AdoptDone,
            Self::AdoptAndOpen => SetupEventKind::AdoptAndOpen,
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
            Self::SetUpElsewhere { .. } => SetupEventKind::SetUpElsewhere,
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
    PortChosen,
    PortGranted,
    PortPickerCancelled,
    PortPickerEmpty,
    ProbeCompleted,
    PortLost,
    WipeAndSetUp,
    UpdateFirmware,
    AdoptDone,
    AdoptAndOpen,
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
    SetUpElsewhere,
}

impl SetupEventKind {
    /// Every event, for the exhaustive transition table. Kept honest the
    /// same way [`SetupStateKind::ALL`](super::SetupStateKind::ALL) is.
    pub const ALL: [Self; 27] = [
        Self::ItsConnected,
        Self::PickBoardFirst,
        Self::ItsPluggedIn,
        Self::Back,
        Self::BoardChosen,
        Self::Confirm,
        Self::PortChosen,
        Self::PortGranted,
        Self::PortPickerCancelled,
        Self::PortPickerEmpty,
        Self::ProbeCompleted,
        Self::PortLost,
        Self::WipeAndSetUp,
        Self::UpdateFirmware,
        Self::AdoptDone,
        Self::AdoptAndOpen,
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
        Self::SetUpElsewhere,
    ];

    pub fn ordinal(self) -> usize {
        match self {
            Self::ItsConnected => 0,
            Self::PickBoardFirst => 1,
            Self::ItsPluggedIn => 2,
            Self::Back => 3,
            Self::BoardChosen => 4,
            Self::Confirm => 5,
            Self::PortChosen => 6,
            Self::PortGranted => 7,
            Self::PortPickerCancelled => 8,
            Self::PortPickerEmpty => 9,
            Self::ProbeCompleted => 10,
            Self::PortLost => 11,
            Self::WipeAndSetUp => 12,
            Self::UpdateFirmware => 13,
            Self::AdoptDone => 14,
            Self::AdoptAndOpen => 15,
            Self::SetUpFresh => 16,
            Self::Retry => 17,
            Self::FlashSucceeded => 18,
            Self::FlashFailed => 19,
            Self::NameEdited => 20,
            Self::ProjectGenerated => 21,
            Self::PushCompleted => 22,
            Self::CloseRequested => 23,
            Self::KeepFlashing => 24,
            Self::Abandon => 25,
            Self::SetUpElsewhere => 26,
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
            Self::PortChosen => "port-chosen",
            Self::PortGranted => "port-granted",
            Self::PortPickerCancelled => "port-picker-cancelled",
            Self::PortPickerEmpty => "port-picker-empty",
            Self::ProbeCompleted => "probe-completed",
            Self::PortLost => "port-lost",
            Self::WipeAndSetUp => "wipe-and-set-up",
            Self::UpdateFirmware => "update-firmware",
            Self::AdoptDone => "adopt-done",
            Self::AdoptAndOpen => "adopt-and-open",
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
            Self::SetUpElsewhere => "set-up-elsewhere",
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
