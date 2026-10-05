//! Where a board stands on firmware updates, as one pure function: the
//! state the card's words ([`super::device_update_words`]) and offers
//! ([`super::device_update_offers`]) are both read from, and what the
//! controller asks before it starts an update with no click.
//!
//! The board's own facts decide through `lpa_update::decide()` — the one
//! decision table, never a second one here — and a running Update activity
//! says what is happening now. One [`UpdateStanding`] variant per row of
//! the update-states spike's table (direction C), plus [`UpdateStanding::Nothing`]
//! for a board with no update story to tell:
//!
//! | Board | Activity | Standing |
//! |---|---|---|
//! | no update facts, no own build facts, a manifest that does not parse, another target | — | `Nothing` (today's card) |
//! | any | an Update, by its stage | `BackingUp`, `Updating`, `Finishing`, `Restoring`, `AnotherDevice` |
//! | `Nothing` (same hashes), or an offer to the same version | — | `UpToDate` |
//! | `OfferUpdate` | — | `Available` |
//! | `NoUpdateForPlayOnly` | — | `PlayOnly` |
//! | `Heal` of the board's own build | — | `Restoring` (starts itself) |
//! | `Heal` of this Studio's build, mid-transfer or on trial | — | `Finishing` (starts itself) |
//! | `Heal` after an update found no copy of the engine | — | `CantGetVersion` |
//! | `ContinueUpdate` | — | `Finishing` (starts itself) |
//! | `Busy` | — | `AnotherDevice` |
//! | `NeedsUsb` | — | `NeedsUsbOnce` |
//! | `ReportCrashing` | — | `KeepsCrashing` |
//! | `RefusedBuild` | — | `RolledBack` |
//! | `BoardIsNewer` | — | `Newer` |
//!
//! A pre-update board (no update channel at all, the roadmap's E9) has no
//! facts, so it is `Nothing` too: its card keeps today's USB flash.

use lpa_devices::view::DeviceView;
use lpa_devices::{FirmwareAge, UpdateFacts, UpdateOutcomeFacts, UpdateStageFacts};
use lpa_update::{BoardView, Decision, HostBuildFacts, HostFacts, decide};
use lpc_access::Tier;
use lpc_update::BoardState;

use super::device_update_route::UpdateLink;
use super::device_update_version::UpdateVersion;
use super::update_build_facts::StoreLatest;

/// Everything a board's standing is read from. A struct so the card, the
/// offers and the controller's no-click start all call the one function.
///
/// The running Update activity and how the last one ended are the view's
/// own ([`DeviceView::activity`]'s `update`, [`DeviceView::last_update_outcome`]).
#[derive(Clone, Copy, Debug)]
pub struct UpdateStandingInputs<'a> {
    /// The board's card, as the model projects it.
    pub view: &'a DeviceView,
    /// The board's latest update facts this window
    /// ([`lpa_devices::Evidence::update_facts`]); `None` when it said
    /// nothing about its firmware's update state.
    pub facts: Option<&'a UpdateFacts>,
    /// This Studio's own build, by its facts; `None` until the update host
    /// has read it.
    pub own: Option<&'a HostBuildFacts>,
    /// The user's tier on this board, when known (a Bluetooth link's
    /// grant); `None` over USB, which is trusted, and while unknown.
    pub tier: Option<Tier>,
    /// The link the board is reached over.
    pub link: UpdateLink,
    /// The firmware store's latest release, when known.
    pub store_latest: Option<&'a StoreLatest>,
}

/// Where a board stands. See the module docs for which row is which.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateStanding {
    /// No update story to tell: the card is today's.
    #[default]
    Nothing,
    /// The board runs this Studio's build.
    UpToDate { version: UpdateVersion },
    /// The board runs another build, and this Studio's can go on it.
    Available {
        board: UpdateVersion,
        to: UpdateVersion,
    },
    /// An update is reading the board's current firmware back first.
    BackingUp {
        board: UpdateVersion,
        to: UpdateVersion,
        percent: Option<u8>,
    },
    /// An update is writing the new firmware.
    Updating {
        board: UpdateVersion,
        to: UpdateVersion,
        link: UpdateLink,
        percent: Option<u8>,
    },
    /// An interrupted update to `to` is being completed (`running`), or is
    /// waiting to be — it starts with no click.
    Finishing {
        board: UpdateVersion,
        to: UpdateVersion,
        percent: Option<u8>,
        running: bool,
    },
    /// The board's own missing firmware is being put back (`running`), or
    /// is waiting to be — it starts with no click.
    Restoring {
        board: UpdateVersion,
        percent: Option<u8>,
        running: bool,
    },
    /// Another device holds the board's transfer; `to` when it is this
    /// Studio's build.
    AnotherDevice {
        board: UpdateVersion,
        to: Option<UpdateVersion>,
        percent: Option<u8>,
    },
    /// The board cannot update over this link yet: one update over USB.
    NeedsUsbOnce {
        board: UpdateVersion,
        to: UpdateVersion,
        link: UpdateLink,
    },
    /// The board's firmware (`board`) keeps crashing, so it stopped
    /// trying. `choices`: the versions this Studio can install instead,
    /// its own first.
    KeepsCrashing {
        board: UpdateVersion,
        choices: Vec<UpdateVersion>,
    },
    /// The board needs `board` to start, and this Studio could not get it.
    /// `own`: this Studio's version, which it can install instead.
    CantGetVersion {
        board: UpdateVersion,
        own: UpdateVersion,
        choices: Vec<UpdateVersion>,
    },
    /// An update to `refused` did not start, so the board went back to
    /// `board` — and refuses that build from now on, so nothing is offered.
    RolledBack {
        board: UpdateVersion,
        refused: UpdateVersion,
    },
    /// The board runs a newer version than this Studio's `own`.
    Newer {
        board: UpdateVersion,
        own: UpdateVersion,
    },
    /// An update is available, but installing it needs the author
    /// password.
    PlayOnly {
        board: UpdateVersion,
        to: UpdateVersion,
    },
}

impl UpdateStanding {
    /// The version the board runs (or needs), as the header names it.
    pub fn board(&self) -> Option<&UpdateVersion> {
        match self {
            Self::Nothing => None,
            Self::UpToDate { version } => Some(version),
            Self::Available { board, .. }
            | Self::BackingUp { board, .. }
            | Self::Updating { board, .. }
            | Self::Finishing { board, .. }
            | Self::Restoring { board, .. }
            | Self::AnotherDevice { board, .. }
            | Self::NeedsUsbOnce { board, .. }
            | Self::KeepsCrashing { board, .. }
            | Self::CantGetVersion { board, .. }
            | Self::RolledBack { board, .. }
            | Self::Newer { board, .. }
            | Self::PlayOnly { board, .. } => Some(board),
        }
    }

    /// Whether an update is being run (by this Studio or another device)
    /// or is about to start by itself: the rows with a lit bar.
    pub fn is_progress(&self) -> bool {
        matches!(
            self,
            Self::BackingUp { .. }
                | Self::Updating { .. }
                | Self::Finishing { .. }
                | Self::Restoring { .. }
                | Self::AnotherDevice { .. }
        )
    }

    /// Whether this Studio is running an update on the board right now.
    pub fn is_running(&self) -> bool {
        match self {
            Self::BackingUp { .. } | Self::Updating { .. } => true,
            Self::Finishing { running, .. } | Self::Restoring { running, .. } => *running,
            _ => false,
        }
    }
}

/// Whether the controller should start the Update activity with no click:
/// a heal (the board waits for its own firmware) or a finish (the board
/// holds this Studio's interrupted transfer), when nothing runs yet. Those
/// are never offers (DS4).
pub fn wants_auto_start(standing: &UpdateStanding) -> bool {
    matches!(
        standing,
        UpdateStanding::Restoring { running: false, .. }
            | UpdateStanding::Finishing { running: false, .. }
    )
}

/// The board's standing. See the module docs.
pub fn update_standing(inputs: &UpdateStandingInputs<'_>) -> UpdateStanding {
    let board_view = inputs
        .facts
        .and_then(|facts| BoardView::from_json(facts.manifest_json.as_bytes()));
    let board_version = inputs.facts.and_then(board_version);
    let own = inputs.own.map(own_version);
    let decision = match (&board_view, inputs.own) {
        (Some(board), Some(build)) => Some(decide(
            board,
            &HostFacts {
                build,
                user_tier: inputs.tier,
                allow_downgrade: false,
            },
        )),
        _ => None,
    };

    // What is happening now outranks what the facts say.
    if let Some(activity) = &inputs.view.activity
        && let Some(update) = &activity.update
    {
        let board = board_version
            .clone()
            .or_else(|| own.clone())
            .unwrap_or_else(|| UpdateVersion::new(""));
        let to = match &update.intent {
            lpa_devices::UpdateIntentFacts::Install { version, .. } => {
                version_named(version, inputs.own, inputs.store_latest)
            }
            _ => own.clone().unwrap_or_else(|| board.clone()),
        };
        let percent = activity.percent;
        let stage = update.stage.or(match decision {
            Some(Decision::Heal { .. }) if !finishing_own(board_view.as_ref(), inputs.own) => {
                Some(UpdateStageFacts::Restoring)
            }
            Some(Decision::Heal { .. } | Decision::ContinueUpdate { .. }) => {
                Some(UpdateStageFacts::Finishing)
            }
            _ => None,
        });
        return match stage {
            Some(UpdateStageFacts::BackingUp) => UpdateStanding::BackingUp { board, to, percent },
            Some(UpdateStageFacts::Restoring) => UpdateStanding::Restoring {
                board,
                percent,
                running: true,
            },
            Some(UpdateStageFacts::Finishing) => UpdateStanding::Finishing {
                board,
                to,
                percent,
                running: true,
            },
            Some(UpdateStageFacts::Waiting) => UpdateStanding::AnotherDevice {
                board,
                to: Some(to),
                percent,
            },
            Some(UpdateStageFacts::Updating) | None => UpdateStanding::Updating {
                board,
                to,
                link: inputs.link,
                percent,
            },
        };
    }

    let (Some(decision), Some(board), Some(own), Some(board_view)) =
        (decision, board_version, own, board_view)
    else {
        return UpdateStanding::Nothing;
    };
    let choices = install_choices(&own, inputs.store_latest, &board_view);
    match decision {
        Decision::Nothing => UpdateStanding::UpToDate { version: own },
        Decision::OfferUpdate { .. } => match board.age_against(&own) {
            // Two builds of one version (a dev tree rebuilt): the same
            // version, so not out of date.
            FirmwareAge::Current => UpdateStanding::UpToDate { version: own },
            _ => UpdateStanding::Available { board, to: own },
        },
        Decision::NoUpdateForPlayOnly => UpdateStanding::PlayOnly { board, to: own },
        Decision::Heal { .. }
            if matches!(
                inputs.view.last_update_outcome,
                Some(UpdateOutcomeFacts::MissingEngine { .. })
            ) =>
        {
            UpdateStanding::CantGetVersion {
                board,
                own,
                choices,
            }
        }
        Decision::Heal { .. } if finishing_own(Some(&board_view), inputs.own) => {
            UpdateStanding::Finishing {
                board,
                to: own,
                percent: None,
                running: false,
            }
        }
        Decision::Heal { .. } => UpdateStanding::Restoring {
            board,
            percent: None,
            running: false,
        },
        Decision::ContinueUpdate { .. } => UpdateStanding::Finishing {
            board,
            to: own,
            percent: transfer_percent(&board_view),
            running: false,
        },
        Decision::Busy { done, total } => {
            let to = board_view
                .updating()
                .filter(|t| inputs.own.is_some_and(|b| t.build_hash == b.build_hash()))
                .map(|_| own);
            UpdateStanding::AnotherDevice {
                board,
                to,
                percent: percent_of(done, total),
            }
        }
        Decision::NeedsUsb { .. } => UpdateStanding::NeedsUsbOnce {
            board,
            to: own,
            link: inputs.link,
        },
        Decision::ReportCrashing { .. } => UpdateStanding::KeepsCrashing { board, choices },
        Decision::RefusedBuild { .. } => UpdateStanding::RolledBack {
            board,
            refused: own,
        },
        Decision::BoardIsNewer => UpdateStanding::Newer { board, own },
        // Another target: moving a board to it is not offered. A reinstall
        // is never decided by the table, only by a person's intent.
        Decision::OtherTarget { .. } | Decision::Reinstall { .. } => UpdateStanding::Nothing,
    }
}

/// The board's version, from its facts.
fn board_version(facts: &UpdateFacts) -> Option<UpdateVersion> {
    let version = facts.version.clone()?;
    Some(UpdateVersion {
        version,
        build_id: facts.build_id.clone(),
    })
}

/// This Studio's version, from its build facts.
fn own_version(own: &HostBuildFacts) -> UpdateVersion {
    UpdateVersion::with_build_id(&own.identity.version, &own.identity.build_id)
}

/// `version` with the build id of the build it names, when this Studio
/// knows that build.
fn version_named(
    version: &str,
    own: Option<&HostBuildFacts>,
    latest: Option<&StoreLatest>,
) -> UpdateVersion {
    own.into_iter()
        .chain(latest.map(|latest| &latest.facts))
        .find(|build| build.identity.version == version)
        .map(own_version)
        .unwrap_or_else(|| UpdateVersion::new(version))
}

/// The versions "Other version…" offers (DS7): this Studio's own, then the
/// store's latest when it is for the board's target and not the same.
fn install_choices(
    own: &UpdateVersion,
    latest: Option<&StoreLatest>,
    board: &BoardView,
) -> Vec<UpdateVersion> {
    let mut choices = vec![own.clone()];
    let target = board.manifest.as_ref().map(|m| m.target.as_str());
    if let Some(latest) = latest
        && Some(latest.target()) == target
        && latest.version() != own.version
    {
        choices.push(own_version(&latest.facts));
    }
    choices
}

/// Whether a heal is the tail of this Studio's own update: the board runs
/// this Studio's core, on trial or mid-transfer, and fetches its engine.
fn finishing_own(board: Option<&BoardView>, own: Option<&HostBuildFacts>) -> bool {
    let (Some(board), Some(own)) = (board, own) else {
        return false;
    };
    board.core_sha256() == Some(own.core.sha256)
        && matches!(
            board.state(),
            Some(BoardState::OnTrial | BoardState::Updating)
        )
}

/// The percent of the transfer the board reports.
fn transfer_percent(board: &BoardView) -> Option<u8> {
    board.updating().and_then(|t| percent_of(t.done, t.total))
}

fn percent_of(done: u32, total: u32) -> Option<u8> {
    if total == 0 {
        return None;
    }
    u8::try_from(u64::from(done.min(total)) * 100 / u64::from(total)).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use lpa_devices::view::{ActivityView, Escape, FirmwareFace, LoadedProject};
    use lpa_devices::{
        ActivityKind, DeviceId, DeviceStatus, UpdateActivityView, UpdateBoardState,
        UpdateIntentFacts,
    };
    use lpa_update::{HostIdentity, HostPieceFacts};
    use lpc_update::{BoardManifest, PieceKind, TransferView, sha256_to_hex};

    use super::*;

    #[test]
    fn no_facts_or_no_own_build_is_no_story() {
        let view = ready_view();
        let facts = facts_of(&board_x());
        let y = studio_y();
        let mut inputs = inputs(&view, Some(&facts), Some(&y));
        inputs.own = None;
        assert_eq!(update_standing(&inputs), UpdateStanding::Nothing);
        inputs.own = Some(&y);
        inputs.facts = None;
        assert_eq!(update_standing(&inputs), UpdateStanding::Nothing);
        let mut garbled = facts_of(&board_x());
        garbled.manifest_json = "not a manifest".to_string();
        inputs.facts = Some(&garbled);
        assert_eq!(update_standing(&inputs), UpdateStanding::Nothing);
    }

    #[test]
    fn each_board_fact_lands_on_its_row() {
        let view = ready_view();
        let y = studio_y();
        let cases: Vec<(BoardManifest, fn(&UpdateStanding) -> bool)> = vec![
            (board_y(), |s| matches!(s, UpdateStanding::UpToDate { .. })),
            (board_x(), |s| matches!(s, UpdateStanding::Available { .. })),
            (newer(), |s| matches!(s, UpdateStanding::Newer { .. })),
            (refused(), |s| {
                matches!(s, UpdateStanding::RolledBack { .. })
            }),
            (crashing(), |s| {
                matches!(s, UpdateStanding::KeepsCrashing { .. })
            }),
            (needs_engine(), |s| {
                matches!(s, UpdateStanding::Restoring { running: false, .. })
            }),
            (on_trial_of_y(), |s| {
                matches!(s, UpdateStanding::Finishing { running: false, .. })
            }),
            (continuing(), |s| {
                matches!(s, UpdateStanding::Finishing { running: false, .. })
            }),
            (busy(), |s| {
                matches!(s, UpdateStanding::AnotherDevice { .. })
            }),
            (old_loader(), |s| {
                matches!(s, UpdateStanding::NeedsUsbOnce { .. })
            }),
        ];
        for (manifest, is_row) in cases {
            let facts = facts_of(&manifest);
            let standing = update_standing(&inputs(&view, Some(&facts), Some(&y)));
            assert!(is_row(&standing), "{manifest:?} → {standing:?}");
        }
    }

    #[test]
    fn play_only_is_its_own_row_and_another_target_tells_no_story() {
        let view = ready_view();
        let y = studio_y();
        let facts = facts_of(&board_x());
        let mut play = inputs(&view, Some(&facts), Some(&y));
        play.tier = Some(Tier::Play);
        assert!(matches!(
            update_standing(&play),
            UpdateStanding::PlayOnly { .. }
        ));
        let mut other = board_x();
        other.target = "esp32c6-8mb-variant".into();
        let facts = facts_of(&other);
        assert_eq!(
            update_standing(&inputs(&view, Some(&facts), Some(&y))),
            UpdateStanding::Nothing
        );
    }

    #[test]
    fn a_heal_that_found_no_copy_is_the_version_studio_cant_get() {
        let mut view = ready_view();
        view.last_update_outcome = Some(UpdateOutcomeFacts::MissingEngine { offline: true });
        let y = studio_y();
        let facts = facts_of(&needs_engine());
        let standing = update_standing(&inputs(&view, Some(&facts), Some(&y)));
        let UpdateStanding::CantGetVersion {
            board,
            own,
            choices,
        } = &standing
        else {
            panic!("{standing:?}");
        };
        assert_eq!(board.version, "2026.10.03-1");
        assert_eq!(own.version, "2026.10.05-2");
        assert_eq!(choices, &vec![own.clone()]);
        assert!(!wants_auto_start(&standing), "it would loop");
    }

    #[test]
    fn other_version_offers_the_stores_latest_when_it_is_for_this_target_and_different() {
        let view = ready_view();
        let y = studio_y();
        let facts = facts_of(&crashing());
        let latest = StoreLatest {
            facts: build("2026.10.07-4", [0xCC; 32], [0xCE; 32]),
        };
        let mut with_latest = inputs(&view, Some(&facts), Some(&y));
        with_latest.store_latest = Some(&latest);
        let UpdateStanding::KeepsCrashing { choices, .. } = update_standing(&with_latest) else {
            panic!();
        };
        let versions: Vec<&str> = choices.iter().map(|c| c.version.as_str()).collect();
        assert_eq!(versions, ["2026.10.05-2", "2026.10.07-4"]);

        let same = StoreLatest { facts: studio_y() };
        with_latest.store_latest = Some(&same);
        let UpdateStanding::KeepsCrashing { choices, .. } = update_standing(&with_latest) else {
            panic!();
        };
        assert_eq!(choices.len(), 1, "the same version is not offered twice");
    }

    #[test]
    fn a_running_update_reads_its_stage_and_percent() {
        let y = studio_y();
        let facts = facts_of(&board_x());
        for (stage, is_row) in [
            (
                Some(UpdateStageFacts::BackingUp),
                (|s: &UpdateStanding| matches!(s, UpdateStanding::BackingUp { .. }))
                    as fn(&UpdateStanding) -> bool,
            ),
            (Some(UpdateStageFacts::Updating), |s| {
                matches!(s, UpdateStanding::Updating { .. })
            }),
            (Some(UpdateStageFacts::Restoring), |s| {
                matches!(s, UpdateStanding::Restoring { running: true, .. })
            }),
            (Some(UpdateStageFacts::Finishing), |s| {
                matches!(s, UpdateStanding::Finishing { running: true, .. })
            }),
            (Some(UpdateStageFacts::Waiting), |s| {
                matches!(s, UpdateStanding::AnotherDevice { .. })
            }),
            (None, |s| matches!(s, UpdateStanding::Updating { .. })),
        ] {
            let view = updating_view(stage, Some(40));
            let standing = update_standing(&inputs(&view, Some(&facts), Some(&y)));
            assert!(is_row(&standing), "{stage:?} → {standing:?}");
            assert!(standing.is_progress());
            assert!(!wants_auto_start(&standing), "it already runs");
        }
    }

    #[test]
    fn heal_and_finish_start_themselves_and_nothing_else_does() {
        let view = ready_view();
        let y = studio_y();
        for (manifest, auto) in [
            (needs_engine(), true),
            (on_trial_of_y(), true),
            (continuing(), true),
            (board_x(), false),
            (crashing(), false),
            (busy(), false),
            (board_y(), false),
        ] {
            let facts = facts_of(&manifest);
            let standing = update_standing(&inputs(&view, Some(&facts), Some(&y)));
            assert_eq!(wants_auto_start(&standing), auto, "{standing:?}");
        }
        assert!(!wants_auto_start(&UpdateStanding::Nothing));
    }

    // ---- Fixtures (shared with the words and offers tests) ---------------

    pub(crate) fn inputs<'a>(
        view: &'a DeviceView,
        facts: Option<&'a UpdateFacts>,
        own: Option<&'a HostBuildFacts>,
    ) -> UpdateStandingInputs<'a> {
        UpdateStandingInputs {
            view,
            facts,
            own,
            tier: None,
            link: UpdateLink::Usb,
            store_latest: None,
        }
    }

    /// This Studio's build Y: `2026.10.05-2`, core `BB…`, engine `BE…`.
    pub(crate) fn studio_y() -> HostBuildFacts {
        build("2026.10.05-2", [0xBB; 32], [0xBE; 32])
    }

    pub(crate) fn build(version: &str, core: [u8; 32], engine: [u8; 32]) -> HostBuildFacts {
        HostBuildFacts::from_parts(
            HostIdentity {
                target: "esp32c6-4mb".into(),
                chip: "esp32c6".into(),
                version: version.into(),
                build_id: format!("{version}+{}", commit_for(version)),
                wire_proto: 36,
                layout: 1,
                min_loader: 1,
            },
            HostPieceFacts {
                sha256: core,
                len: 20_000,
            },
            HostPieceFacts {
                sha256: engine,
                len: 40_000,
            },
        )
    }

    /// A stable fake commit per version (a dev version is its own commit).
    fn commit_for(version: &str) -> String {
        if version.bytes().all(|b| b.is_ascii_hexdigit()) {
            return format!("{version:0<12}");
        }
        match version {
            "2026.10.03-1" => "a41c9e2d11f0".to_string(),
            "2026.10.05-2" => "626a1b851aaa".to_string(),
            "2026.10.07-4" => "c08d1f3eeee0".to_string(),
            _ => "f00dfac00000".to_string(),
        }
    }

    /// The board on X: `2026.10.03-1`, running.
    pub(crate) fn board_x() -> BoardManifest {
        manifest("2026.10.03-1", [0xAA; 32], [0xAE; 32])
    }

    pub(crate) fn manifest(version: &str, core: [u8; 32], engine: [u8; 32]) -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: version.into(),
            build_id: format!("{version}+{}", commit_for(version)),
            wire_proto: 36,
            core_sha256: sha256_to_hex(&core),
            core_len: 18_000,
            engine_sha256: sha256_to_hex(&engine),
            engine_len: Some(38_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }

    /// The board already on Y, by its hashes.
    pub(crate) fn board_y() -> BoardManifest {
        manifest("2026.10.05-2", [0xBB; 32], [0xBE; 32])
    }

    pub(crate) fn newer() -> BoardManifest {
        manifest("2026.10.07-4", [0xCC; 32], [0xCE; 32])
    }

    pub(crate) fn refused() -> BoardManifest {
        BoardManifest {
            refused_build: Some(studio_y().build_hash()),
            ..board_x()
        }
    }

    pub(crate) fn crashing() -> BoardManifest {
        BoardManifest {
            state: BoardState::EngineCrashing,
            ..board_x()
        }
    }

    pub(crate) fn needs_engine() -> BoardManifest {
        BoardManifest {
            state: BoardState::NeedsEngine,
            engine_len: None,
            ..board_x()
        }
    }

    pub(crate) fn on_trial_of_y() -> BoardManifest {
        BoardManifest {
            state: BoardState::OnTrial,
            engine_len: None,
            ..board_y()
        }
    }

    pub(crate) fn continuing() -> BoardManifest {
        BoardManifest {
            state: BoardState::Updating,
            transfer: Some(TransferView {
                kind: PieceKind::Core,
                done: 14_000,
                total: 20_000,
                busy: false,
                build_hash: studio_y().build_hash(),
            }),
            ..board_x()
        }
    }

    pub(crate) fn busy() -> BoardManifest {
        BoardManifest {
            state: BoardState::Updating,
            transfer: Some(TransferView {
                kind: PieceKind::Core,
                done: 8_000,
                total: 20_000,
                busy: true,
                build_hash: studio_y().build_hash(),
            }),
            ..board_x()
        }
    }

    pub(crate) fn old_loader() -> BoardManifest {
        BoardManifest {
            loader: 0,
            ..board_x()
        }
    }

    /// The device model's mirror of `m`, as the evidence carries it.
    pub(crate) fn facts_of(m: &BoardManifest) -> UpdateFacts {
        UpdateFacts {
            state: match m.state {
                BoardState::Running => UpdateBoardState::Running,
                BoardState::NeedsEngine => UpdateBoardState::NeedsEngine,
                BoardState::EngineCrashing => UpdateBoardState::EngineCrashing,
                BoardState::Updating => UpdateBoardState::Updating,
                BoardState::OnTrial => UpdateBoardState::OnTrial,
                BoardState::Unknown => UpdateBoardState::Unknown,
            },
            version: Some(m.version.clone()),
            target: Some(m.target.clone()),
            build_id: Some(m.build_id.clone()),
            transfer: None,
            refused_build: m.refused_build,
            manifest_json: String::from_utf8(m.to_json()).unwrap(),
        }
    }

    /// A Ready LightPlayer on an open USB port, idle.
    pub(crate) fn ready_view() -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Porch lights".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: Some("60:55:f9:0a:0b:0c".to_string()),
            detected_chip: Some("esp32c6".to_string()),
            board_id: None,
            firmware_face: FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
                age: FirmwareAge::Unknown,
            },
            remembered_firmware: None,
            degraded: None,
            loaded_project: LoadedProject::Empty,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            last_update_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            update_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }

    /// The same board with an Update activity at `stage`.
    pub(crate) fn updating_view(
        stage: Option<UpdateStageFacts>,
        percent: Option<u8>,
    ) -> DeviceView {
        let mut view = ready_view();
        view.activity = Some(ActivityView {
            kind: ActivityKind::Update,
            label: "Updating…".to_string(),
            percent,
            cancellable: stage.is_none_or(UpdateStageFacts::allows_cancel),
            cancel_requested: false,
            layout: None,
            update: Some(UpdateActivityView {
                intent: UpdateIntentFacts::Install {
                    version: "2026.10.05-2".to_string(),
                    allow_downgrade: false,
                },
                stage,
                done: 0,
                total: 0,
                outcome: None,
                between_legs: false,
            }),
        });
        view.escapes = if stage.is_none_or(UpdateStageFacts::allows_cancel) {
            vec![Escape::Cancel, Escape::Disconnect, Escape::Forget]
        } else {
            vec![Escape::Disconnect, Escape::Forget]
        };
        view
    }
}
