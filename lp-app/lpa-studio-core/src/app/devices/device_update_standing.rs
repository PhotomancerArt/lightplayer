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
//! | `OfferUpdate` or `NeedsUsb` of a tree-store build, to a board whose hello does not list `fs.tree` | — | `NeedsFilesMoved` |
//! | over Wi‑Fi, a release older than the link's first, or the last update heard nothing on the update channel | — | `NotOverWifiYet` |
//! | `ReportCrashing` | — | `KeepsCrashing` |
//! | `RefusedBuild` | — | `RolledBack` |
//! | `BoardIsNewer` | — | `Newer` |
//! | any | an Update in its project stage | `UpgradingProjects` |
//! | up to date (or no update story), its projects being upgraded with no Update running | — | `UpgradingProjects` |
//! | up to date (or no update story), the project stage held back | — | `ProjectsUnfinished` |
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
use super::project_stage_status::{ProjectStageStatus, ProjectsHeldBack};
use super::store_lookups::StoreLookups;
use super::update_build_facts::{StoreLatest, StoreReleases};

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
    /// The firmware store's release index, when known. The standing never
    /// reads it; the card's version choices do
    /// ([`super::UpdateOfferFacts::read`]).
    pub store_releases: Option<&'a StoreReleases>,
    /// The releases looked up by exact version, when any were. Like the
    /// index, read only by the card's version choices.
    pub store_lookups: Option<&'a StoreLookups>,
    /// The custom build picked from files ("From a file…"), when one is.
    /// Read only by the card's version choices.
    pub file_build: Option<&'a HostBuildFacts>,
    /// The board's filesystem is the tree store (its hello lists `fs.tree`):
    /// a tree-store build may go to it over the air. `false` for a board on
    /// littlefs — and for one that has not said hello this window.
    pub board_tree_store: bool,
    /// Where the board's project stage stands when no Update runs it (M3
    /// P11): running with no click, or how it was held back. `None` when
    /// nothing is pending.
    pub projects: Option<&'a ProjectStageStatus>,
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
    /// The new firmware's last part is being installed (`running`), or an
    /// interrupted update to `to` is waiting to be completed — it starts
    /// with no click.
    Finishing {
        board: UpdateVersion,
        to: UpdateVersion,
        percent: Option<u8>,
        running: bool,
        /// This Studio did not see the update start: it found the board
        /// half-way and is completing it (the update's no-click `Auto`
        /// intent, or a finish still waiting to start). `false` for the
        /// ordinary last phase of an update this Studio started.
        resumed: bool,
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
    /// This Studio's build (`to`) mounts the tree store and the board holds
    /// littlefs: its files move to the new filesystem over USB — the card's
    /// USB update, which backs them up first — never over the air (the
    /// tree-store cutover, AD3).
    NeedsFilesMoved {
        board: UpdateVersion,
        to: UpdateVersion,
        link: UpdateLink,
    },
    /// The board's firmware (`board`) keeps crashing, so it stopped
    /// trying. The versions it can take instead are the card's choices
    /// ([`super::InstallChoice`]).
    KeepsCrashing { board: UpdateVersion },
    /// The board needs `board` to start, and this Studio could not get it.
    /// `own`: this Studio's version, which it can install instead.
    CantGetVersion {
        board: UpdateVersion,
        own: UpdateVersion,
    },
    /// An update to `refused` did not start, so the board went back to
    /// `board` — and refuses that build from now on (another version can
    /// still be installed).
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
    /// Over Bluetooth or Wi‑Fi (`link`), the board could update, but this
    /// Studio's build cannot be installed over the air (a single image, or a
    /// build with no update files): nothing to install from here. Read by
    /// [`super::UpdateOfferFacts::read`] off the route, never by
    /// [`update_standing`].
    NoWirelessBuild {
        board: UpdateVersion,
        link: UpdateLink,
    },
    /// The board's new engine reads a newer project format than projects it
    /// stores, and this Studio is rewriting them (the update's last stage,
    /// or one started with no click on a board found that way): `done` of
    /// `total` projects.
    UpgradingProjects {
        board: UpdateVersion,
        done: u32,
        total: u32,
    },
    /// Projects older than the board's engine format are still on it, and
    /// this Studio cannot finish them: `why`.
    ProjectsUnfinished {
        board: UpdateVersion,
        why: ProjectsHeldBack,
    },
    /// Over Wi‑Fi, the board's firmware is from before updates over Wi‑Fi:
    /// it announced the update channel, then said nothing on it when asked
    /// (the last update ended [`UpdateOutcomeFacts::NotOverWifi`]). It
    /// updates over USB or Bluetooth until it has been updated once.
    NotOverWifiYet {
        board: UpdateVersion,
        /// Wi‑Fi on its network, or through the relay.
        link: UpdateLink,
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
            | Self::NeedsFilesMoved { board, .. }
            | Self::KeepsCrashing { board, .. }
            | Self::CantGetVersion { board, .. }
            | Self::RolledBack { board, .. }
            | Self::Newer { board, .. }
            | Self::PlayOnly { board, .. }
            | Self::NoWirelessBuild { board, .. }
            | Self::NotOverWifiYet { board, .. }
            | Self::UpgradingProjects { board, .. }
            | Self::ProjectsUnfinished { board, .. } => Some(board),
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
                | Self::UpgradingProjects { .. }
        )
    }

    /// How far along the whole update is, as the one number every place
    /// says (the card's surface, its Firmware row, the editor's popover):
    /// the new firmware is the first half and its last part the second, so
    /// the number never restarts at "2 of 2". A backup, a restore and
    /// another device's update are one step each. `None` while the step has
    /// no number yet, and for a standing that is no update under way.
    pub fn whole_percent(&self) -> Option<u8> {
        let half = |percent: Option<u8>, second: bool| {
            percent.map(|percent| (u32::from(percent.min(100)) / 2 + u32::from(second) * 50) as u8)
        };
        match self {
            Self::Updating { percent, .. } => half(*percent, false),
            Self::Finishing { percent, .. } => half(*percent, true),
            Self::BackingUp { percent, .. }
            | Self::Restoring { percent, .. }
            | Self::AnotherDevice { percent, .. } => percent.map(|percent| percent.min(100)),
            Self::UpgradingProjects { done, total, .. } => percent_of(*done, *total),
            _ => None,
        }
    }

    /// Whether this Studio is running an update on the board right now.
    pub fn is_running(&self) -> bool {
        match self {
            Self::BackingUp { .. } | Self::Updating { .. } | Self::UpgradingProjects { .. } => true,
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
                // Only the no-click intent finishes what it did not start.
                resumed: update.intent.is_auto(),
            },
            Some(UpdateStageFacts::Waiting) => UpdateStanding::AnotherDevice {
                board,
                to: Some(to),
                percent,
            },
            // The engine now running is the one the update installed.
            Some(UpdateStageFacts::UpgradingProjects) => UpdateStanding::UpgradingProjects {
                board: to,
                done: update.done,
                total: update.total,
            },
            Some(UpdateStageFacts::Updating) | None => UpdateStanding::Updating {
                board,
                to,
                link: inputs.link,
                percent,
            },
        };
    }

    let firmware = firmware_standing(inputs, decision, board_version, own, board_view);
    projects_standing(inputs, &firmware).unwrap_or(firmware)
}

/// The board's project stage, when no Update runs it: it outranks a board
/// that is up to date or tells no update story, since it is what is left to
/// do; anything the firmware still needs comes first.
fn projects_standing(
    inputs: &UpdateStandingInputs<'_>,
    firmware: &UpdateStanding,
) -> Option<UpdateStanding> {
    let status = inputs.projects?;
    if !matches!(
        firmware,
        UpdateStanding::Nothing | UpdateStanding::UpToDate { .. } | UpdateStanding::Newer { .. }
    ) {
        return None;
    }
    let board = firmware
        .board()
        .cloned()
        .or_else(|| inputs.facts.and_then(board_version))
        .unwrap_or_else(|| UpdateVersion::new(""));
    Some(match status {
        ProjectStageStatus::Running { done, total } => UpdateStanding::UpgradingProjects {
            board,
            done: *done,
            total: *total,
        },
        ProjectStageStatus::HeldBack(why) => UpdateStanding::ProjectsUnfinished {
            board,
            why: why.clone(),
        },
    })
}

/// The board's firmware standing, from its facts (no Update running).
fn firmware_standing(
    inputs: &UpdateStandingInputs<'_>,
    decision: Option<Decision>,
    board_version: Option<UpdateVersion>,
    own: Option<UpdateVersion>,
    board_view: Option<BoardView>,
) -> UpdateStanding {
    // Over Wi‑Fi, a board that announced the update channel and then said
    // nothing on it (or a known release from before the link's first one,
    // which would): nothing more is offered there that would only hang. A
    // dev build has no order, so for it only the silence tells.
    if inputs.link.is_wifi()
        && let Some(board) = board_version.clone()
        && (inputs.view.last_update_outcome == Some(UpdateOutcomeFacts::NotOverWifi)
            || predates_link(inputs.link, &board))
    {
        return UpdateStanding::NotOverWifiYet {
            board,
            link: inputs.link,
        };
    }

    let (Some(decision), Some(board), Some(own), Some(board_view)) =
        (decision, board_version, own, board_view)
    else {
        return UpdateStanding::Nothing;
    };
    // A tree-store build is never sent over the air to a board on littlefs:
    // its files move over USB first.
    if inputs.own.is_some_and(|build| build.needs_tree_store)
        && !inputs.board_tree_store
        && matches!(
            decision,
            Decision::OfferUpdate { .. } | Decision::NeedsUsb { .. }
        )
    {
        return UpdateStanding::NeedsFilesMoved {
            board,
            to: own,
            link: inputs.link,
        };
    }
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
            UpdateStanding::CantGetVersion { board, own }
        }
        Decision::Heal { .. } if finishing_own(Some(&board_view), inputs.own) => {
            UpdateStanding::Finishing {
                board,
                to: own,
                percent: None,
                running: false,
                resumed: true,
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
            resumed: true,
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
        Decision::ReportCrashing { .. } => UpdateStanding::KeepsCrashing { board },
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

/// Whether `board` is a release older than the first one that serves the
/// update channel over `link`; a dev build or an unnamed first release is not.
fn predates_link(link: UpdateLink, board: &UpdateVersion) -> bool {
    link.first_update_release()
        .is_some_and(|first| board.age_against(&UpdateVersion::new(first)) == FirmwareAge::Older)
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
    use lpa_devices::view::{Escape, FirmwareFace, LoadedProject};
    use lpa_devices::{DeviceId, DeviceStatus};
    use lpc_update::BoardManifest;

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

    /// Over Wi‑Fi and the relay a known release older than the link's first
    /// is `NotOverWifiYet` before any update is tried; a release from it on,
    /// or a dev build (no order), is not.
    #[test]
    fn a_release_before_a_links_first_update_release_stands_not_over_wifi_yet() {
        let view = ready_view();
        let y = studio_y();
        let standing_of = |version: &str, link: UpdateLink| {
            let facts = facts_of(&manifest(version, [0xAA; 32], [0xAE; 32]));
            let mut inputs = inputs(&view, Some(&facts), Some(&y));
            inputs.link = link;
            update_standing(&inputs)
        };
        let not_yet = |s: &UpdateStanding, want: UpdateLink| matches!(s, UpdateStanding::NotOverWifiYet { link, .. } if *link == want);
        let (wifi, relay) = (UpdateLink::Wifi, UpdateLink::Relay);
        // Before both; between them; from the relay's on.
        assert!(not_yet(&standing_of("2026.10.08-1", wifi), wifi));
        assert!(not_yet(&standing_of("2026.10.08-1", relay), relay));
        assert!(matches!(
            standing_of("2026.10.08-2", wifi),
            UpdateStanding::Newer { .. }
        ));
        assert!(not_yet(&standing_of("2026.10.08-2", relay), relay));
        assert!(matches!(
            standing_of("2026.10.08-9", relay),
            UpdateStanding::Newer { .. }
        ));
        // A dev build has no order; USB and Bluetooth are not Wi‑Fi links.
        assert!(matches!(
            standing_of("5eb70a7c2", relay),
            UpdateStanding::Available { .. }
        ));
        assert!(matches!(
            standing_of("2026.10.08-1", UpdateLink::Usb),
            UpdateStanding::Newer { .. }
        ));
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
        let UpdateStanding::CantGetVersion { board, own } = &standing else {
            panic!("{standing:?}");
        };
        assert_eq!(board.version, "2026.10.03-1");
        assert_eq!(own.version, "2026.10.05-2");
        assert!(!wants_auto_start(&standing), "it would loop");
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

    /// The last phase of an update this Studio started is not "resumed";
    /// finishing what it found half-way (the no-click intent, or a finish
    /// still waiting to start) is.
    #[test]
    fn finishing_is_resumed_only_when_this_studio_did_not_start_the_update() {
        use lpa_devices::UpdateIntentFacts;
        let y = studio_y();
        let facts = facts_of(&board_x());
        let finishing = |view: &DeviceView| {
            let standing = update_standing(&inputs(view, Some(&facts), Some(&y)));
            let UpdateStanding::Finishing {
                resumed, running, ..
            } = standing
            else {
                panic!("not finishing: {standing:?}");
            };
            (running, resumed)
        };
        // A press of Update: Install, and its engine phase is ordinary.
        let pressed = updating_view(Some(UpdateStageFacts::Finishing), Some(70));
        assert_eq!(finishing(&pressed), (true, false));
        // Found half-way on connect: Auto, running.
        let found = super::super::device_update_fixtures::with_update_intent(
            ready_view(),
            UpdateIntentFacts::Auto,
            Some(UpdateStageFacts::Finishing),
            Some(70),
        );
        assert_eq!(finishing(&found), (true, true));
        // Found half-way, not started yet.
        for manifest in [on_trial_of_y(), continuing()] {
            let waiting = facts_of(&manifest);
            let standing = update_standing(&inputs(&ready_view(), Some(&waiting), Some(&y)));
            assert!(
                matches!(
                    standing,
                    UpdateStanding::Finishing {
                        running: false,
                        resumed: true,
                        ..
                    }
                ),
                "{standing:?}"
            );
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

    /// AD3: a tree-store build never goes over the air to a board on
    /// littlefs; the board stands "files move", over USB and wireless alike,
    /// and the same build is an ordinary update to a board on the store.
    #[test]
    fn a_tree_store_build_for_a_littlefs_board_stands_files_moved() {
        let view = ready_view();
        let tree_y = studio_y().with_tree_store(true);
        let facts = facts_of(&board_x());
        for link in [UpdateLink::Usb, UpdateLink::Bluetooth] {
            let mut on_littlefs = inputs(&view, Some(&facts), Some(&tree_y));
            on_littlefs.link = link;
            assert!(
                matches!(
                    update_standing(&on_littlefs),
                    UpdateStanding::NeedsFilesMoved { link: l, .. } if l == link
                ),
                "{link:?}"
            );
            let mut on_store = on_littlefs;
            on_store.board_tree_store = true;
            assert!(matches!(
                update_standing(&on_store),
                UpdateStanding::Available { .. }
            ));
        }
        // A littlefs build to a littlefs board: today's update.
        let y = studio_y();
        assert!(matches!(
            update_standing(&inputs(&view, Some(&facts), Some(&y))),
            UpdateStanding::Available { .. }
        ));
        // A board that cannot update over its link yet still moves its files
        // (both need the cable).
        let old = facts_of(&old_loader());
        assert!(matches!(
            update_standing(&inputs(&view, Some(&old), Some(&tree_y))),
            UpdateStanding::NeedsFilesMoved { .. }
        ));
        // A heal puts back the board's own build: untouched by the rule.
        let healing = facts_of(&needs_engine());
        assert!(wants_auto_start(&update_standing(&inputs(
            &view,
            Some(&healing),
            Some(&tree_y)
        ))));
    }

    // ---- Fixtures (shared with the words and offers tests) ---------------

    pub(crate) fn inputs<'a>(
        view: &'a DeviceView,
        facts: Option<&'a UpdateFacts>,
        own: Option<&'a HostBuildFacts>,
    ) -> UpdateStandingInputs<'a> {
        UpdateStandingInputs {
            board_tree_store: false,
            view,
            facts,
            own,
            tier: None,
            link: UpdateLink::Usb,
            store_latest: None,
            store_releases: None,
            store_lookups: None,
            file_build: None,
            projects: None,
        }
    }

    // The manifests and builds are the story fixtures' own, so a story
    // and a test read the same board.
    pub(crate) use super::super::device_update_fixtures::{
        board_x, board_y, build, busy, continuing, crashing, facts_of, manifest, needs_engine,
        newer, old_loader, on_trial_of_y, refused, studio_y,
    };

    /// A Ready LightPlayer on an open USB port, idle.
    pub(crate) fn ready_view() -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Porch lights".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            heard_age_ms: None,
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
            held_elsewhere: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }

    /// The same board with an Update activity at `stage`.
    pub(crate) fn updating_view(
        stage: Option<UpdateStageFacts>,
        percent: Option<u8>,
    ) -> DeviceView {
        super::super::device_update_fixtures::with_update_activity(ready_view(), stage, percent)
    }
}
