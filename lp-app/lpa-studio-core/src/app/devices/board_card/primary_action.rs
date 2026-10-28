//! The name bar's one primary action (`docs/ux/anatomy/board-card.md`,
//! "Name bar and primary"): the most likely thing the person wants to do
//! (Yona, 2026-10-17), even when it repeats a row's own button.
//!
//! The primary follows the card's surface ([`super::board_surface`]): the
//! surface says what is going on and its second sentence says this button
//! aloud, so both read one state ([`surface_state`]) and can never
//! disagree. Before the state, two rules: while a Connect is under way the
//! primary is the spinner alone, and while this tab's session is on the
//! board (its card, or the editor's docked card) it is Done.
//!
//! | Surface state | Primary | Offer | Icon |
//! |---|---|---|---|
//! | Connecting, or a take-over under way | the spinner alone | — | — |
//! | The session on the board (connected, reconnecting, in the editor) | Done | `done` | check |
//! | Updating, installing, erasing, sending, removing, identifying | nothing (the surface and the busy row say it) | — | — |
//! | Open in another tab, taken by another tab, a take-over that failed | Connect here | `take-over` | the held link's |
//! | Busy in another tab | nothing (the surface: Connect here when it's done) | — | — |
//! | Someone else connected, a Wi‑Fi connect failed | Retry | `connect-wifi` / `connect-relay` | retry |
//! | The last Connect failed | Retry | `connect` | retry |
//! | Not answering (a board that said hello went quiet) | Retry | `retry` | retry |
//! | A stand-in, off | Power on | `reconnect` | play |
//! | Online at lightplayer.app, no link here | Connect | `connect`, else the road's own offer (`connect-relay` for a board never met here) | cloud (or Wi‑Fi's, when that road comes first) |
//! | Offline: Wi‑Fi first, then lightplayer.app, then its cable | Connect | `connect`, else the road's own offer | wifi / cloud / the link's |
//! | Offline, nothing reaches it | nothing | — | — |
//! | A send, an update, an install, an erase or a removal failed, while its verb is offered again | Retry (the row's) | `push` / `update-firmware` / `flash` / `erase` / `remove-project` | retry |
//! | Its files wait for newer firmware | Update firmware | `update-firmware` | firmware |
//! | Its files were lost | Restore, else Reset | `restore-files`, else `start-fresh` | upload / add |
//! | A move of its files stopped half-way | Resume update | `finish-update` | download |
//! | Keeps crashing | Reinstall firmware | `reinstall-firmware` | firmware |
//! | Needs a version Studio can't get, or only its core runs | Install firmware | `install-firmware`: Studio's own at one press, or the version its list picks | firmware |
//! | An older LightPlayer | Update firmware | `update-firmware`, else `flash` | firmware |
//! | From before the hello, no LightPlayer, download mode, says nothing (it never said hello) | Install firmware | `flash`, one press | firmware |
//! | Locked | Unlock (the password sheet) | `unlock` | lock |
//! | Its port closed | Connect | `connect` | the link's |
//! | A shader error | Edit project | `edit` (its lock on a play-only link) | edit |
//! | Runs no project | Add a project (the project pick) | `push` | add |
//! | All well, or its picture too large | Connect, granted at any tier | `connect` | the link's |
//! | The layout question open, its store refused | nothing (its row and details hold the answer) | — | — |
//!
//! **No dead ends** (P7). When nothing can be pressed there is no button
//! at all — never a greyed verb: an offer its link refuses is no primary,
//! and the surface says the refusal in the button's place ("Firmware
//! updates need USB."); a fix that isn't offered here is said the same
//! way, why and what to do instead ("Connect over USB or Bluetooth to
//! install it."). Work that failed hands over to the board's own state
//! once its verb is no longer offered. A Connect that waits ("No
//! project", "Project not known") is no button either: the Project row
//! says why.
//!
//! The primary says its verb and its object ("Update firmware"); a row's
//! own button says the verb alone ("Update"), because the row already
//! names its object (P12). A name bar too narrow for the board's name
//! beside the whole word says the verb alone too ([`UiCardAction::verb_alone`]):
//! the surface's sentence names the object.
//!
//! A new board's primary is [`pending_primary`]: Install firmware once it
//! has settled on needing firmware (Update firmware on an older
//! LightPlayer), else the spinner while it identifies.
//!
//! Connect means one thing (the board card ADR, §4): this board's panel,
//! here, on its card, reaching the board first when it must. Edit is the
//! Project row's action ([`super::project_bar::edit_action`]), and while the
//! board is connected, the panel foot's.

use lpa_devices::ActivityKind;

use lpa_devices::view::{FirmwareFace, PendingLinkView};

use super::bar_work::retry;
use super::board_card_input::{BoardCardInput, link_icon, offer_at};
use super::board_surface::{offline_road, road_link, runs_no_project, surface_state};
use super::project_bar::edit_action;
use super::ui_bar_work::UiBarWork;
use super::ui_board_surface::SurfaceState;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_name_bar::UiPrimary;
use crate::{OfferPath, UiOffer};

/// A link row's state while a Connect is under way ("USB connecting");
/// the primary is the spinner alone then.
pub const CONNECTING: &str = "connecting";

/// The primary of a ready board that runs no project: the next step.
pub const ADD_A_PROJECT: &str = "Add a project";

/// The primary's word on a take-over: Connect, here, in this tab.
pub const CONNECT_HERE: &str = "Connect here";

/// A board's primary; `None` when nothing can be pressed.
pub(crate) fn primary_action(
    input: &BoardCardInput<'_>,
    _work: Option<&UiBarWork>,
) -> Option<UiPrimary> {
    let state = surface_state(input);
    match state {
        // No Cancel while connecting (Q10): the open is bounded by the
        // device request deadline, a held Connect by its grace.
        SurfaceState::Connecting => return Some(UiPrimary::working("Connecting")),
        SurfaceState::MovingHere => {
            return input
                .take_over
                .map(|over| UiPrimary::working(over.words.clone()));
        }
        _ => {}
    }
    // The session is on the board: on its card, or docked in the editor.
    if input.editor_holds_it {
        return input.offer("done").map(|done| offer(done_action(done)));
    }
    state_action(input, state)
        .filter(|action| action.refused.is_none())
        .map(offer)
}

/// The offer `state` asks the person to press, in the primary's words —
/// refused or not: the surface says a refusal in the button's place. `None`
/// for a state with nothing to press.
pub(crate) fn state_action(
    input: &BoardCardInput<'_>,
    state: SurfaceState,
) -> Option<UiCardAction> {
    use SurfaceState::*;
    let view = input.view;
    let icon = link_icon(input.link_kind());
    let press = |verb: &str, word: &str, icon: &str| {
        input
            .offer(verb)
            .map(|offer| UiCardAction::press(offer, word).with_icon(icon))
    };
    match state {
        AllWell | PictureTooLarge => press("connect", "Connect", icon),
        Connected | InTheEditor | Reconnecting => input.offer("done").map(done_action),
        Connecting | MovingHere | Updating | Installing | Erasing | Sending | Removing
        | Identifying | AnotherTabBusy | ConfirmationRequired | DataUnreadable => None,
        AnotherTab | TakenByAnotherTab | MoveFailed => {
            press("take-over", CONNECT_HERE, link_icon(input.held_link()))
        }
        SomeoneElse | NetworkFailed => {
            let verb = match input
                .wifi_connect
                .is_some_and(|connect| connect.through_relay)
            {
                true => "connect-relay",
                false => "connect-wifi",
            };
            press(verb, "Retry", "retry")
        }
        ConnectFailed => press("connect", "Retry", "retry"),
        NotAnswering => press("retry", "Retry", "retry"),
        PoweredOff => press("reconnect", "Power on", "play"),
        Offline | OfflineReachable | OnlineAtCloud => offline_road(input).map(|(road, offer)| {
            UiCardAction::press(offer, "Connect").with_icon(link_icon(road_link(input, road)))
        }),
        Disconnected => press("connect", "Connect", icon),
        SendFailed => retry(input, ActivityKind::Push),
        UpdateFailed => retry(input, ActivityKind::Update),
        InstallFailed => retry(input, ActivityKind::Flash),
        EraseFailed => retry(input, ActivityKind::Erase),
        RemoveFailed => retry(input, ActivityKind::RemoveProject),
        UpdateRequired => update_firmware(input),
        DataLost => input
            .offer("restore-files")
            .filter(|_| view.firmware_blocked.is_none())
            .map(|restore| UiCardAction::press(restore, "Restore").with_icon("upload"))
            .or_else(|| press("start-fresh", "Reset", "add")),
        UpdateInterrupted => {
            press("finish-update", RESUME_UPDATE, "download").map(|resume| UiCardAction {
                refused: resume.refused.or_else(|| view.firmware_blocked.clone()),
                ..resume
            })
        }
        Crashing => press("reinstall-firmware", REINSTALL_FIRMWARE, "firmware"),
        // Studio's own build: one press, or — where the row's details list
        // the versions — the version that list picks for the person.
        CantGetVersion | Incomplete => input
            .offer("install-firmware")
            .map(|install| UiCardAction::press(install, INSTALL_FIRMWARE).with_icon("firmware")),
        OldFirmware => update_firmware(input).or_else(|| {
            input.offer("flash").map(|flash| UiCardAction {
                word: UPDATE_FIRMWARE.to_string(),
                ..install(flash)
            })
        }),
        PreHelloFirmware | NoFirmware | DownloadMode | Silent => {
            install_firmware(input).or_else(|| {
                update_firmware(input).map(|update| UiCardAction {
                    word: INSTALL_FIRMWARE.to_string(),
                    ..update
                })
            })
        }
        Locked => input.offer("unlock").map(|unlock| {
            UiCardAction::press(unlock, "Unlock")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet)
        }),
        // Edit opens what it runs; a board that runs nothing (its startup
        // project didn't load) is given one.
        ProjectError => edit_action(input)
            .map(|edit| UiCardAction {
                word: EDIT_PROJECT.to_string(),
                ..edit
            })
            .or_else(|| add_a_project(input))
            .or_else(|| press("connect", "Connect", icon)),
        NoProject => add_a_project(input),
    }
}

/// The primary's words that name their object (P12): verb and object.
const UPDATE_FIRMWARE: &str = "Update firmware";
const INSTALL_FIRMWARE: &str = "Install firmware";
const REINSTALL_FIRMWARE: &str = "Reinstall firmware";
const RESUME_UPDATE: &str = "Resume update";
const EDIT_PROJECT: &str = "Edit project";

/// The verb alone of a primary that names its object: what a name bar too
/// narrow for the board's name (about twelve characters) beside the whole
/// word says instead. The surface's sentence already names the object.
fn verb_alone(word: &str) -> Option<&'static str> {
    Some(match word {
        UPDATE_FIRMWARE => "Update",
        INSTALL_FIRMWARE => "Install",
        REINSTALL_FIRMWARE => "Reinstall",
        RESUME_UPDATE => "Resume",
        EDIT_PROJECT => "Edit",
        ADD_A_PROJECT => "Add",
        CONNECT_HERE => "Connect",
        _ => return None,
    })
}

/// Update firmware: `update-firmware`, one press.
fn update_firmware(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    let update = input.offer("update-firmware")?;
    Some(UiCardAction::press(update, UPDATE_FIRMWARE).with_icon("firmware"))
}

/// Done: the session's own verb.
fn done_action(done: &UiOffer) -> UiCardAction {
    UiCardAction::press(done, "Done").with_icon("check")
}

/// Add a project: the primary of a ready, idle board that runs no project
/// — `push` with the project pick, the same pick the Project row's Add
/// opens. Exactly where the surface says "This board has no project yet."
/// ([`super::board_surface::runs_no_project`]): it said it runs nothing,
/// or the library knows it plays nothing. `None` when `push` is not
/// offered. Refused, it is no primary, and the surface says the refusal in
/// the button's place.
pub(crate) fn add_a_project(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    let view = input.view;
    if !runs_no_project(input) {
        return None;
    }
    Some(
        UiCardAction::press(input.offer("push")?, ADD_A_PROJECT)
            .with_icon("add")
            .drawn(UiActionDraw::ProjectPick {
                board_id: view.board_id.clone(),
            }),
    )
}

/// A new board's primary: Install firmware once it settled on needing
/// firmware (the board pick, filtered by the chip its boot banner named;
/// Update firmware on an older LightPlayer), else the spinner alone while
/// it identifies (the Hardware row says "identifying").
pub(crate) fn pending_primary(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
) -> Option<UiPrimary> {
    if let Some(action) = pending_action(pending, board, offers) {
        return action.refused.is_none().then(|| offer(action));
    }
    (!pending.needs_firmware()).then(|| UiPrimary::working("Identifying"))
}

/// The offer a new board's surface asks for, refused or not: its Flash.
pub(crate) fn pending_action(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
) -> Option<UiCardAction> {
    let flash = offer_at(offers, board, "flash")?;
    let word = match pending.firmware_face {
        FirmwareFace::OlderLightPlayer { .. } => UPDATE_FIRMWARE,
        _ => INSTALL_FIRMWARE,
    };
    Some(UiCardAction {
        word: word.to_string(),
        ..install(flash)
    })
}

/// Install firmware on a board with no LightPlayer: the `flash` offer, one
/// press — the primary's, and the Firmware row's to-do ("none · Install").
/// The board is read in its bootloader before anything is written; nobody
/// picks a board first (plan `lp2025/2026-10-28-0025-install-without-board-pick`).
pub(crate) fn install_firmware(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    input.offer("flash").map(install)
}

/// Install firmware: the `flash` offer, in the card's words (P12: install
/// what?).
fn install(flash: &UiOffer) -> UiCardAction {
    UiCardAction::press(flash, INSTALL_FIRMWARE).with_icon("firmware")
}

/// The primary pressing `action`, with its verb alone for a narrow name
/// bar.
fn offer(action: UiCardAction) -> UiPrimary {
    UiPrimary::Offer(match verb_alone(&action.word) {
        Some(verb) => action.with_verb_alone(verb),
        None => action,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{Escape, FirmwareFace, LoadedProject};
    use lpa_devices::{ActivityKind, HoldLevel, HoldVia};

    use super::super::board_connection::BoardConnection;
    use super::super::card_fixtures::{CardFixture, activity};
    use super::*;
    use crate::app::access::{UiDeviceAccess, UiUnlockOffer};
    use crate::app::devices::take_over_state::UiTakeOver;
    use crate::app::devices::ui_link_kind::UiLinkKind;

    /// The no-project surface always has its button: a board the library
    /// knows plays nothing, whatever its last report said, gets Add a
    /// project too, and the surface says the button aloud (the mismatch
    /// that left "This board has no project yet." with no primary).
    #[test]
    fn a_board_that_runs_no_project_always_gets_add_a_project() {
        for loaded in [
            LoadedProject::Empty,
            LoadedProject::Running {
                label: "porch".to_string(),
            },
        ] {
            let mut fixture = CardFixture::ready();
            fixture.view.loaded_project = loaded.clone();
            fixture.plays = crate::app::devices::board_plays::BoardPlays::Nothing;
            let card = super::super::board_card_builder::board_card(&fixture.input());
            assert_eq!(card.surface.state, SurfaceState::NoProject, "{loaded:?}");
            assert_eq!(
                card.name_bar.primary.as_ref().and_then(UiPrimary::word),
                Some(ADD_A_PROJECT),
                "{loaded:?}"
            );
            let notice = card.surface.notice.expect("the surface says it");
            assert_eq!(notice.first, "This board has no project yet.");
            assert_eq!(
                notice.second.as_deref(),
                Some("Add a project to play on it.")
            );
        }
    }

    #[test]
    fn a_connecting_board_says_connecting_with_its_links_icon() {
        // The session opening (the connected record, opening), and a Connect
        // held for the board: the same row.
        for session_on_it in [true, false] {
            let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
            fixture.editor_holds_it = session_on_it;
            fixture.connection = BoardConnection::Connecting;
            assert_eq!(
                primary_action(&fixture.input(), None),
                Some(UiPrimary::working("Connecting")),
                "the spinner alone; session on it: {session_on_it}"
            );
            assert!(
                fixture
                    .offers()
                    .iter()
                    .all(|offer| !offer.path.to_string().ends_with("/connect")),
                "no second Connect while one waits"
            );
        }
    }

    #[test]
    fn the_session_on_the_board_is_done() {
        // On its card (connected), and docked in the editor.
        for docked in [false, true] {
            let mut fixture = CardFixture::ready();
            fixture.editor_holds_it = true;
            fixture.docked = docked;
            fixture.connection = BoardConnection::Connected;
            let action = offered(&mut fixture);
            assert_eq!(action.word, "Done", "docked: {docked}");
            assert_eq!(action.icon.as_deref(), Some("check"));
            assert_eq!(path(&action), "done");
            assert_eq!(action.draw, UiActionDraw::Press);
        }
        // A dropped link held for the board: still the session's, still Done.
        let mut held = CardFixture::offline();
        held.editor_holds_it = true;
        held.connection = BoardConnection::Reconnecting;
        assert_eq!(path(&offered(&mut held)), "done");
    }

    /// P2: a busy board has no button — the row doing the work says it,
    /// once.
    #[test]
    fn a_busy_board_has_no_primary() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Update,
            "Updating firmware",
            Some(42),
        ));
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    /// No board pick: Install is one press (plan
    /// `lp2025/2026-10-28-0025-install-without-board-pick`).
    #[test]
    fn a_board_that_needs_firmware_installs_on_one_press() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::Blank;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.view.status = DeviceStatus::NeedsAttention;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Install firmware");
        assert_eq!(action.icon.as_deref(), Some("firmware"));
        assert_eq!(path(&action), "flash");
        assert_eq!(action.draw, UiActionDraw::Press);
    }

    #[test]
    fn a_locked_board_unlocks_with_the_sheet() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth).locked();
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Unlock");
        assert_eq!(action.icon.as_deref(), Some("lock"));
        assert_eq!(path(&action), "unlock");
        assert_eq!(action.draw, UiActionDraw::Sheet);
    }

    #[test]
    fn an_attached_board_connects_over_its_link() {
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::Attached;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Connect");
        assert_eq!(action.icon.as_deref(), Some("usb"));
        assert_eq!(path(&action), "connect");
    }

    #[test]
    fn an_offline_board_connects_by_its_road_wifi_first_then_the_cloud_then_its_cable() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        fixture.relay = true;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.word.as_str(), action.icon.as_deref(), path(&action)),
            ("Connect", Some("wifi"), "connect".to_string())
        );
        fixture.wifi_address = false;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.icon.as_deref(), path(&action)),
            (Some("cloud"), "connect".to_string())
        );
        fixture.relay = false;
        let action = offered(&mut fixture);
        assert_eq!(
            (action.icon.as_deref(), path(&action)),
            (Some("usb"), "connect".to_string())
        );
        // A board with no registry row has no session to open: its road's
        // own offer reaches it.
        fixture.uid = None;
        assert_eq!(path(&offered(&mut fixture)), "reconnect");
    }

    #[test]
    fn an_offline_stand_in_powers_on() {
        let mut fixture = CardFixture::offline();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Power on");
        assert_eq!(action.icon.as_deref(), Some("play"));
        assert_eq!(path(&action), "reconnect");
    }

    /// Offline with no way in: no button; the surface says how long it
    /// has been away.
    #[test]
    fn an_offline_board_nothing_reaches_has_no_primary() {
        let mut fixture = CardFixture::offline().over(UiLinkKind::Bluetooth);
        fixture.view.escapes = vec![Escape::Forget];
        fixture.last_seen_at = Some(fixture.now - 15.0 * 86_400.0);
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    /// The dead end of 2026-10-17: an older LightPlayer on USB, its board
    /// known. Its primary is the update, pressed by path, one click.
    #[test]
    fn an_older_lightplayer_updates_its_firmware() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::OlderLightPlayer { proto: Some(33) };
        fixture.view.status = DeviceStatus::NeedsAttention;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Update firmware");
        assert_eq!(action.icon.as_deref(), Some("firmware"));
        assert_eq!(path(&action), "update-firmware");
        assert_eq!(action.draw, UiActionDraw::Press);
        let update = fixture.offers_at("update-firmware");
        assert!(
            update.consequence().is_routine(),
            "the update keeps the project and the files: {:?}",
            update.consequence()
        );
        // Its board not known: the same one press — the install reads the
        // chip in the board's bootloader.
        fixture.view.board_id = None;
        fixture.view.detected_chip = None;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Update firmware");
        assert_eq!(path(&action), "update-firmware");
        assert_eq!(action.draw, UiActionDraw::Press);
    }

    #[test]
    fn a_board_not_answering_retries() {
        let mut fixture = CardFixture::not_answering();
        let action = offered(&mut fixture);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Retry", "retry".to_string())
        );
    }

    #[test]
    fn a_crashing_board_reinstalls_and_lost_files_restore() {
        let mut crashing = CardFixture::ready();
        crashing
            .extra_offers
            .push(extra(&crashing, "reinstall-firmware"));
        crashing.view.firmware_face = FirmwareFace::CoreOnly {
            version: Some("2026.10.08-9".to_string()),
            state: lpa_devices::UpdateBoardState::EngineCrashing,
        };
        crashing.view.status = DeviceStatus::NeedsAttention;
        let action = offered(&mut crashing);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Reinstall firmware", "reinstall-firmware".to_string())
        );

        let mut lost = CardFixture::ready();
        lost.extra_offers.push(extra(&lost, "restore-files"));
        lost.extra_offers.push(extra(&lost, "start-fresh"));
        let action = offered(&mut lost);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Restore", "restore-files".to_string())
        );
        lost.without_offer("restore-files");
        lost.extra_offers.push(extra(&lost, "restore-from-file"));
        let action = offered(&mut lost);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Reset", "start-fresh".to_string())
        );
    }

    #[test]
    fn a_ready_running_board_connects() {
        let mut fixture = CardFixture::ready();
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Connect");
        assert_eq!(action.icon.as_deref(), Some("usb"));
        assert_eq!(path(&action), "connect");
        assert_eq!(action.draw, UiActionDraw::Press);
        assert_eq!(action.refused, None);
    }

    #[test]
    fn a_play_only_board_connects_the_play_password_is_enough() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            unlock: Some(UiUnlockOffer::PlayOnly),
            ..UiDeviceAccess::default()
        });
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Connect");
        assert_eq!(action.icon.as_deref(), Some("bluetooth"));
        assert_eq!(path(&action), "connect");
    }

    /// No project, no panel: the primary is the next step, Add a project
    /// (`push` with the project pick), never a greyed Connect.
    #[test]
    fn a_ready_board_running_no_project_adds_a_project() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Add a project");
        assert_eq!(action.icon.as_deref(), Some("add"));
        assert_eq!(path(&action), "push");
        assert_eq!(action.refused, None);
        assert!(matches!(
            action.draw,
            UiActionDraw::ProjectPick { board_id: Some(_) }
        ));
    }

    /// With nothing to push (no `push`, or `push` refused) there is no
    /// button: the Project row says "none".
    #[test]
    fn a_ready_board_with_nothing_to_add_has_no_primary() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.without_offer("push");
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    /// A ready board that has not said what it runs: no button while the
    /// Project row waits.
    #[test]
    fn a_ready_board_that_has_not_said_has_no_primary() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_remove_project = false;
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    #[test]
    fn a_failed_connect_retries_its_connect() {
        let mut fixture = CardFixture::ready();
        fixture.connection = BoardConnection::Failed {
            reason: "the board did not answer".to_string(),
        };
        let action = offered(&mut fixture);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Retry", "connect".to_string())
        );
    }

    #[test]
    fn a_new_board_spins_until_it_needs_firmware() {
        let mut pending = pending_view();
        let board = super::super::card_fixtures::board();
        assert_eq!(
            pending_primary(&pending, &board, &[]),
            Some(UiPrimary::working("Identifying"))
        );
        pending.firmware_face = FirmwareFace::Blank;
        let offers = crate::pending_link_offers(
            &pending,
            &board,
            crate::app::devices::device_reset_reach::ResetReach::Lines,
        );
        let Some(UiPrimary::Offer(action)) = pending_primary(&pending, &board, &offers) else {
            panic!("Install firmware once it settles");
        };
        assert_eq!(action.word, "Install firmware");
        assert_eq!(path(&action), "flash");
    }

    #[test]
    fn a_board_another_tab_holds_connects_by_taking_it_over() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        let action = offered(&mut fixture);
        assert_eq!(action.word, "Connect here");
        assert_eq!(action.icon.as_deref(), Some("usb"));
        assert_eq!(path(&action), "take-over");
        assert_eq!(action.draw, UiActionDraw::Press);
        assert_eq!(action.refused, None);
        let take_over = fixture.offers_at("take-over");
        assert!(
            take_over.consequence().is_routine(),
            "nothing is open there"
        );

        // An editor open over there makes it Undoable: the error tint is
        // the warning, and the card draws the same Connect.
        fixture.view.held_elsewhere.as_mut().unwrap().level = HoldLevel::Open;
        assert_eq!(path(&offered(&mut fixture)), "take-over");
        let take_over = fixture.offers_at("take-over");
        assert!(!take_over.consequence().is_routine() && !take_over.consequence().arms());
    }

    #[test]
    fn a_held_board_sits_above_attached_and_offline() {
        // Status Attached would be Connect over `connect`; held, `connect`
        // is not offered and the take-over is the primary.
        let mut attached = CardFixture::held(HoldLevel::Watching);
        assert_eq!(attached.view.status, DeviceStatus::Attached);
        assert_eq!(path(&offered(&mut attached)), "take-over");
        // Offline (no port here at all) reads the same.
        let mut offline = CardFixture::held(HoldLevel::Watching);
        offline.view.status = DeviceStatus::Offline;
        assert_eq!(path(&offered(&mut offline)), "take-over");
    }

    /// Busy in the other tab: no button; the USB row says it.
    #[test]
    fn a_board_the_holder_is_busy_with_has_no_primary() {
        let mut fixture = CardFixture::held(HoldLevel::Busy("Updating \u{b7} 42%".to_string()));
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    /// A take-over under way is the board being reached: the spinner alone.
    #[test]
    fn a_take_over_under_way_spins() {
        for words in ["Asking the other tab\u{2026}", "Opening\u{2026}"] {
            let mut fixture = CardFixture::held(HoldLevel::Watching);
            fixture.take_over = Some(UiTakeOver {
                words: words.to_string(),
                failed: false,
            });
            assert_eq!(
                primary_action(&fixture.input(), None),
                Some(UiPrimary::working(words))
            );
        }
    }

    #[test]
    fn a_take_over_that_failed_leaves_connect_to_press_again() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        fixture.take_over = Some(UiTakeOver {
            words: "That tab didn't answer".to_string(),
            failed: true,
        });
        assert_eq!(path(&offered(&mut fixture)), "take-over");
    }

    #[test]
    fn a_board_held_on_the_network_names_the_network_link() {
        let mut fixture = CardFixture::held(HoldLevel::Watching).over(UiLinkKind::Wifi);
        fixture.view.held_elsewhere.as_mut().unwrap().via = HoldVia::Network;
        let action = offered(&mut fixture);
        assert_eq!(action.icon.as_deref(), Some("wifi"));
        assert_eq!(path(&action), "take-over");
        let mut relay = CardFixture::held(HoldLevel::Watching).over(UiLinkKind::Relay);
        relay.view.held_elsewhere.as_mut().unwrap().via = HoldVia::Network;
        assert_eq!(offered(&mut relay).icon.as_deref(), Some("cloud"));
    }

    #[test]
    fn a_held_board_with_nobody_to_ask_has_no_primary() {
        // The controller offers `take-over` only with a hold edge; without
        // one the card has nothing to press, and the USB row says who has
        // the board.
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        fixture.without_offer("take-over");
        assert_eq!(primary_action(&fixture.input(), None), None);
    }

    #[test]
    fn a_board_this_tab_has_open_is_not_held_from_here() {
        // Held on the network, read over this tab's own USB cable: Connect
        // here is not gated on the other tab's hold (the ruling on P2–P3),
        // because the link it would use is this tab's own.
        let mut fixture = CardFixture::ready();
        fixture.view.held_elsewhere = Some(lpa_devices::HeldElsewhere {
            via: HoldVia::Network,
            level: HoldLevel::Watching,
            taken_from_here: false,
        });
        let action = offered(&mut fixture);
        assert_eq!(path(&action), "connect");
        assert_eq!(action.refused, None);
    }

    fn offered(fixture: &mut CardFixture) -> UiCardAction {
        match primary_action(&fixture.input(), None) {
            Some(UiPrimary::Offer(action)) => action,
            other => panic!("an offer, not {other:?}"),
        }
    }

    fn path(action: &UiCardAction) -> String {
        let path = action.offer.to_string();
        let prefix = "devices/mac-a0f26287b48c/";
        assert!(path.starts_with(prefix), "{path}");
        path[prefix.len()..].to_string()
    }

    /// `verb` published on the fixture's board beside the device's own, as
    /// the controller publishes its layout and update verbs; what it
    /// presses is the board's own published `identify` (the card only
    /// reads that the verb is there).
    fn extra(fixture: &CardFixture, verb: &str) -> UiOffer {
        let identify = fixture.clone().offers_at("identify");
        UiOffer::new(fixture.board.clone().child(verb), "upload", identify.action)
    }

    pub(crate) fn pending_view() -> PendingLinkView {
        PendingLinkView {
            link: lpa_devices::LinkId(1),
            device: lpa_devices::DeviceId(9),
            title: "New board".to_string(),
            state_label: "Identifying…".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: FirmwareFace::Unknown,
            detected_chip: Some("esp32c6".to_string()),
            mac: None,
            firmware_blocked: None,
            usb: None,
            held_by_tab: false,
            escapes: vec![Escape::Forget],
        }
    }
}
