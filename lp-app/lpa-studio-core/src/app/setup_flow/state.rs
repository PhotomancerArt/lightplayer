//! Where the setup flow stands. Design: `docs/design/device-setup-flow.md` §2.
//!
//! Every state below is a card state (the wizard IS the device card), so
//! the data each one carries is exactly what that card has to render — no
//! UI types, no view models.

use super::verdict::BoardProbe;

/// Why CONNECT_INTRO is being shown, so the copy can escalate toward the
/// "pick my board first" CTA instead of repeating itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectHint {
    /// First arrival.
    #[default]
    None,
    /// The browser's port chooser was dismissed.
    PickerClosed,
    /// The chooser offered nothing — the strongest push toward BOARD_FIRST
    /// (driver guidance is usually the real answer).
    NoPortsSeen,
    /// The port went away mid-flow.
    Disconnected,
}

/// How a flow ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// ✕ before anything was written.
    Cancelled,
    /// Abandoned during or after a flash: the card must say "incomplete
    /// flash — needs re-flash". An incomplete flash is never trusted.
    IncompleteFlash,
    /// ✕ at PROVISION, after a flash that landed: the board is alive and
    /// running our firmware, so the port is KEPT and the board stays on
    /// the roster (G2 walk, 2026-08-05 — releasing it left a
    /// just-flashed board reading "not connected", with a Reconnect that
    /// then had to fight for the port it had just been given). Nothing is
    /// marked: an incomplete flash is a different reason, and this flash
    /// was complete.
    LeftConnected,
    /// ALREADY_LP's "Done": the board was adopted — its sighting is
    /// recorded and the flow is over, WITHOUT a lens attach (G2 follow-up,
    /// 2026-08-05: adopt does not navigate; setup does). The port is
    /// deliberately NOT released — "it joins your roster as it is" means
    /// the board is still there, on its own card, when the flow ends.
    Adopted,
}

/// BOARD_PICK's data. `probe` is `None` on a target that needs no connect
/// (the simulator) — that absence, not a kind check, is what makes the
/// picker unfiltered there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardPickState {
    pub probe: Option<BoardProbe>,
    /// The board the user has picked so far, if any.
    pub selected: Option<String>,
    /// This pick replaces existing firmware — WLED being wiped, or a
    /// LightPlayer board being set up fresh. The confirmation warns.
    pub replaces_firmware: bool,
}

/// How far PROVISION has got. The F2 edge PROVISION → DEVICE_HOME is the
/// last of these; the earlier ones keep the state (design §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionPhase {
    /// The name field is editable and nothing has been written.
    Editing,
    /// `GenerateProject` is in flight.
    Generating,
    /// The registry write and the push are in flight.
    Pushing,
}

/// PROVISION's data.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionState {
    pub board_id: String,
    /// The probe that got here; `None` on a no-connect target.
    pub probe: Option<BoardProbe>,
    /// The derived-then-editable device name. Empty on a target that
    /// cannot be renamed (the simulator shows no field).
    pub name: String,
    pub phase: ProvisionPhase,
    /// Set once the generator answers.
    pub project_uid: Option<String>,
}

/// The setup flow's position.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupState {
    ConnectIntro {
        hint: ConnectHint,
    },
    BoardFirst {
        /// The board picked here pre-seeds BOARD_PICK when the probed chip
        /// turns out to be compatible.
        chosen: Option<String>,
    },
    PortPicking {
        preseeded_board: Option<String>,
    },
    Probing {
        preseeded_board: Option<String>,
    },
    BoardPick(BoardPickState),
    WledFound {
        probe: BoardProbe,
    },
    AlreadyLp {
        probe: BoardProbe,
    },
    ProbeFailed {
        probe: BoardProbe,
    },
    Flashing {
        board_id: String,
        probe: Option<BoardProbe>,
        /// 1 on the first attempt.
        attempt: u32,
    },
    FlashFailed {
        board_id: String,
        probe: Option<BoardProbe>,
        attempt: u32,
        detail: String,
    },
    AbandonGuard {
        board_id: String,
        probe: Option<BoardProbe>,
        attempt: u32,
    },
    Provision(ProvisionState),
    DeviceHome {
        project_uid: Option<String>,
        /// True when this is the adopt landing (ALREADY_LP → Done): the
        /// device kept its own project and nothing was written.
        adopted: bool,
    },
    Closed {
        reason: CloseReason,
    },
}

impl SetupState {
    pub fn kind(&self) -> SetupStateKind {
        match self {
            Self::ConnectIntro { .. } => SetupStateKind::ConnectIntro,
            Self::BoardFirst { .. } => SetupStateKind::BoardFirst,
            Self::PortPicking { .. } => SetupStateKind::PortPicking,
            Self::Probing { .. } => SetupStateKind::Probing,
            Self::BoardPick(_) => SetupStateKind::BoardPick,
            Self::WledFound { .. } => SetupStateKind::WledFound,
            Self::AlreadyLp { .. } => SetupStateKind::AlreadyLp,
            Self::ProbeFailed { .. } => SetupStateKind::ProbeFailed,
            Self::Flashing { .. } => SetupStateKind::Flashing,
            Self::FlashFailed { .. } => SetupStateKind::FlashFailed,
            Self::AbandonGuard { .. } => SetupStateKind::AbandonGuard,
            Self::Provision(_) => SetupStateKind::Provision,
            Self::DeviceHome { .. } => SetupStateKind::DeviceHome,
            Self::Closed { .. } => SetupStateKind::Closed,
        }
    }

    /// The probe this state carries, when it carries one.
    pub fn probe(&self) -> Option<&BoardProbe> {
        match self {
            Self::BoardPick(pick) => pick.probe.as_ref(),
            Self::WledFound { probe } | Self::AlreadyLp { probe } | Self::ProbeFailed { probe } => {
                Some(probe)
            }
            Self::Flashing { probe, .. }
            | Self::FlashFailed { probe, .. }
            | Self::AbandonGuard { probe, .. } => probe.as_ref(),
            Self::Provision(provision) => provision.probe.as_ref(),
            _ => None,
        }
    }

    /// Whether a port grant is outstanding in this state — what decides
    /// whether closing has to release one. Only meaningful on a target that
    /// needs connecting; the caller gates on that capability.
    ///
    /// PROVISION counts: on hardware the board is still on the wire, and
    /// the push goes over it.
    pub fn holds_port(&self) -> bool {
        matches!(
            self.kind(),
            SetupStateKind::Probing
                | SetupStateKind::BoardPick
                | SetupStateKind::WledFound
                | SetupStateKind::AlreadyLp
                | SetupStateKind::ProbeFailed
                | SetupStateKind::Flashing
                | SetupStateKind::FlashFailed
                | SetupStateKind::AbandonGuard
                | SetupStateKind::Provision
        )
    }
}

/// A state's identity without its data — the axis of the transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupStateKind {
    ConnectIntro,
    BoardFirst,
    PortPicking,
    Probing,
    BoardPick,
    WledFound,
    AlreadyLp,
    ProbeFailed,
    Flashing,
    FlashFailed,
    AbandonGuard,
    Provision,
    DeviceHome,
    Closed,
}

impl SetupStateKind {
    /// Every state, for the exhaustive transition table. Kept honest by
    /// [`Self::ordinal`]: a new variant fails to compile there, and the
    /// bijection test then fails until it is listed here too.
    pub const ALL: [Self; 14] = [
        Self::ConnectIntro,
        Self::BoardFirst,
        Self::PortPicking,
        Self::Probing,
        Self::BoardPick,
        Self::WledFound,
        Self::AlreadyLp,
        Self::ProbeFailed,
        Self::Flashing,
        Self::FlashFailed,
        Self::AbandonGuard,
        Self::Provision,
        Self::DeviceHome,
        Self::Closed,
    ];

    pub fn ordinal(self) -> usize {
        match self {
            Self::ConnectIntro => 0,
            Self::BoardFirst => 1,
            Self::PortPicking => 2,
            Self::Probing => 3,
            Self::BoardPick => 4,
            Self::WledFound => 5,
            Self::AlreadyLp => 6,
            Self::ProbeFailed => 7,
            Self::Flashing => 8,
            Self::FlashFailed => 9,
            Self::AbandonGuard => 10,
            Self::Provision => 11,
            Self::DeviceHome => 12,
            Self::Closed => 13,
        }
    }

    /// Whether the probe's VERDICT has landed — the recognition moment.
    ///
    /// This is the seam the wizard's placement turns on (G2 follow-up,
    /// 2026-08-05). Before it, the bound board is a port and nothing else:
    /// no verdict, so no identity, so its live card cannot be merged with
    /// the registry row a KNOWN board already has, and rendering both
    /// would be two representations of one board. So the pre-verdict
    /// states keep the wizard standalone and stand the bound session's row
    /// down. From the verdict on, the recognition is in hand and the
    /// wizard rides the board's own card.
    ///
    /// DEVICE_HOME counts (it is reached through a verdict); CLOSED does
    /// not — a terminal flow places nothing.
    pub fn has_verdict(self) -> bool {
        matches!(
            self,
            Self::BoardPick
                | Self::WledFound
                | Self::AlreadyLp
                | Self::ProbeFailed
                | Self::Flashing
                | Self::FlashFailed
                | Self::AbandonGuard
                | Self::Provision
                | Self::DeviceHome
        )
    }

    /// Stable label for event-log records and test failure messages
    /// (extend, do not rename).
    pub fn label(self) -> &'static str {
        match self {
            Self::ConnectIntro => "connect-intro",
            Self::BoardFirst => "board-first",
            Self::PortPicking => "port-picking",
            Self::Probing => "probing",
            Self::BoardPick => "board-pick",
            Self::WledFound => "wled-found",
            Self::AlreadyLp => "already-lp",
            Self::ProbeFailed => "probe-failed",
            Self::Flashing => "flashing",
            Self::FlashFailed => "flash-failed",
            Self::AbandonGuard => "abandon-guard",
            Self::Provision => "provision",
            Self::DeviceHome => "device-home",
            Self::Closed => "closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_kind_is_listed_exactly_once() {
        // The guard that keeps the transition table exhaustive: a new
        // variant must be added to `ordinal` (compile error) and then to
        // `ALL` (this test) before the table can be complete.
        let mut seen: Vec<usize> = SetupStateKind::ALL.iter().map(|k| k.ordinal()).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..SetupStateKind::ALL.len()).collect::<Vec<_>>());
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<&str> = SetupStateKind::ALL.iter().map(|k| k.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), SetupStateKind::ALL.len());
    }

    #[test]
    fn the_verdict_seam_starts_at_the_states_that_carry_a_probe() {
        // The placement rule reads this: pre-verdict the wizard is
        // standalone, from the verdict on it rides the board's card. The
        // seam has to line up with "the state carries a probe", because a
        // probe IS the recognition — the two must not drift.
        for kind in SetupStateKind::ALL {
            let carries_probe = matches!(
                kind,
                SetupStateKind::BoardPick
                    | SetupStateKind::WledFound
                    | SetupStateKind::AlreadyLp
                    | SetupStateKind::ProbeFailed
                    | SetupStateKind::Flashing
                    | SetupStateKind::FlashFailed
                    | SetupStateKind::AbandonGuard
                    | SetupStateKind::Provision
            );
            let expected = carries_probe || kind == SetupStateKind::DeviceHome;
            assert_eq!(
                kind.has_verdict(),
                expected,
                "{} is on the wrong side of the verdict seam",
                kind.label()
            );
        }
    }

    #[test]
    fn only_the_states_after_a_port_grant_hold_one() {
        assert!(
            !SetupState::ConnectIntro {
                hint: ConnectHint::None
            }
            .holds_port()
        );
        assert!(
            !SetupState::PortPicking {
                preseeded_board: None
            }
            .holds_port()
        );
        assert!(
            SetupState::Probing {
                preseeded_board: None
            }
            .holds_port()
        );
        assert!(SetupState::BoardPick(BoardPickState::default()).holds_port());
    }
}
