//! The name bar's one primary action (`docs/ux/anatomy/board-card.md`,
//! "Name bar and primary"; the connected plan's P4 table).
//!
//! In order, first match wins:
//!
//! | Board | Primary | Offer | Icon |
//! |---|---|---|---|
//! | Connecting (the session opening, or a Connect held for the board) | the spinner alone ("USB · connecting" is the Link row's) | — | — |
//! | The session is on it (its card, or the editor's docked card) | Done | `done` | check |
//! | Busy | nothing (the busy row says it) | — | — |
//! | Another tab holds it | Connect (it is the take-over) | `take-over` | the held link's |
//! | …and is busy with it | nothing (the Link row says it) | — | — |
//! | …a take-over is under way | the spinner alone | — | — |
//! | Needs firmware, `flash` offered | Install firmware (the board pick) | `flash` | firmware |
//! | Locked | Unlock (the password sheet) | `unlock` | lock |
//! | Attached (the port is there, closed) | Connect | `connect` | the link's |
//! | Offline stand-in (a sim, an in-tab board) | Power on | `reconnect` | play |
//! | Offline, reachable | Connect: Wi‑Fi first, then the cloud, then the cable | `connect` | wifi / cloud / the link's, by the road |
//! | Offline, nothing reaches it | nothing ("Offline · 2 weeks" is the Link row's) | — | — |
//! | Ready, running, granted at any tier | Connect | `connect` | the link's |
//! | Ready, running no project | Add a project (the project pick) | `push` | add |
//! | …with nothing to push, or not said what it runs | nothing (the Project row says it) | — | — |
//!
//! **No dead ends, and no repeats** (P2, P7). When nothing can be pressed
//! there is no button at all — never a greyed verb, and never the row's
//! words said again on the name bar: the busy, away or waiting row says
//! why, once. While the board is being reached or identified, the primary
//! is the spinner alone ([`UiPrimary::Working`]). With nothing on the board
//! to connect to, the primary is the next useful step: Add a project, the
//! same `push` the Project row would carry, which then does not repeat it.
//!
//! A new board's primary is [`pending_primary`]: Install firmware once it
//! has settled on needing firmware, else the spinner while it identifies.
//!
//! Connect means one thing (the board card ADR, §4): this board's panel,
//! here, on its card, reaching the board first when it must. Edit is the
//! Project row's action ([`super::project_bar::edit_action`]), and while the
//! board is connected, the panel's All controls row's.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{LoadedProject, PendingLinkView};

use super::bar_work::board_pick;
use super::board_card_input::{BoardCardInput, link_icon, offer_at};
use super::board_connection::BoardConnection;
use super::held_board::held_primary;
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_name_bar::UiPrimary;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::core::data_kinds::duration_words;
use crate::{ActionEnablement, OfferPath, UiOffer};

/// The Link row's state while a Connect is under way ("USB · connecting");
/// the primary is the spinner alone then.
pub const CONNECTING: &str = "connecting";

/// The primary of a ready board that runs no project: the next step.
pub const ADD_A_PROJECT: &str = "Add a project";

/// A board's primary; `None` when nothing can be pressed.
pub(crate) fn primary_action(
    input: &BoardCardInput<'_>,
    _work: Option<&UiBarWork>,
) -> Option<UiPrimary> {
    let view = input.view;
    let link = input.link_kind();
    let icon = link_icon(link);
    // No Cancel while connecting (Q10): the open is bounded by the device
    // request deadline, a held Connect by its grace. The Link row says how.
    if *input.connection == BoardConnection::Connecting {
        return Some(UiPrimary::working("Connecting"));
    }
    // The session is on the board: on its card, or docked in the editor.
    if input.editor_holds_it {
        return input
            .offer("done")
            .map(|done| offer(UiCardAction::press(done, "Done").with_icon("check")));
    }
    // Busy: the row doing the work says it (P2).
    if view.activity.is_some() {
        return None;
    }
    // A board another tab holds is neither attached nor offline from here:
    // Connect is the take-over.
    if let Some(primary) = held_primary(input) {
        return primary;
    }
    if view.needs_firmware()
        && let Some(flash) = input.offer("flash")
    {
        return Some(offer(install(flash).drawn(board_pick(input))));
    }
    if input.locked()
        && let Some(unlock) = input.offer("unlock")
    {
        return Some(offer(
            UiCardAction::press(unlock, "Unlock")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet),
        ));
    }
    if view.status == DeviceStatus::Attached
        && let Some(connect) = input.offer("connect")
    {
        return Some(offer(
            UiCardAction::press(connect, "Connect").with_icon(icon),
        ));
    }
    if view.status == DeviceStatus::Offline {
        return offline_primary(input, link);
    }
    // Ready: with nothing on the board, the next step; else Connect,
    // granted at any tier.
    if let Some(add) = add_a_project(input) {
        return Some(offer(add));
    }
    // Connect, when it can be pressed; a Connect that waits ("No project",
    // "Project not known") is no button: the Project row says why.
    input
        .offer("connect")
        .filter(|connect| connect.action.meta().enablement == ActionEnablement::Enabled)
        .map(|connect| offer(UiCardAction::press(connect, "Connect").with_icon(icon)))
}

/// Add a project: the primary of a ready, idle board that runs no project
/// — `push` with the project pick, as the project bar would draw it (the
/// bar then does not repeat it). `None` when the board runs something or
/// has not said, and when `push` is not offered or is refused (the primary
/// then says "No project").
pub(crate) fn add_a_project(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    let view = input.view;
    let ready = matches!(view.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    if !ready || !input.idle() || view.loaded_project != LoadedProject::Empty {
        return None;
    }
    let action = UiCardAction::press(input.offer("push")?, ADD_A_PROJECT)
        .with_icon("add")
        .drawn(UiActionDraw::ProjectPick {
            board_id: view.board_id.clone(),
        });
    action.refused.is_none().then_some(action)
}

/// An offline board's primary: a stand-in powers on; a board Studio can
/// reach connects, reaching it over Wi‑Fi first, then through
/// lightplayer.app, then by its cable (`connect`, whose icon is the road it
/// takes); with none of those, nothing — the Link row says how long it has
/// been away.
fn offline_primary(input: &BoardCardInput<'_>, link: UiLinkKind) -> Option<UiPrimary> {
    if input.stand_in()
        && let Some(reconnect) = input.offer("reconnect")
    {
        return Some(offer(
            UiCardAction::press(reconnect, "Power on").with_icon("play"),
        ));
    }
    if let Some(connect) = input.offer("connect") {
        return Some(offer(
            UiCardAction::press(connect, "Connect").with_icon(connect.icon.clone()),
        ));
    }
    // A board with no registry row has no session to open; its roads' own
    // offers still reach it.
    for (verb, icon) in [
        ("connect-wifi", "wifi"),
        ("connect-relay", "cloud"),
        ("reconnect", link_icon(link)),
    ] {
        if let Some(found) = input.offer(verb) {
            return Some(offer(UiCardAction::press(found, "Connect").with_icon(icon)));
        }
    }
    None
}

/// "Offline · 2 weeks", from the registry's last sighting; "Offline" when
/// Studio has none.
pub(crate) fn offline_words(last_seen_at: Option<f64>, now: f64) -> String {
    match last_seen_at {
        Some(seen) => format!("Offline · {}", duration_words(now - seen)),
        None => "Offline".to_string(),
    }
}

/// A new board's primary: Install firmware once it settled on needing
/// firmware (the board pick, filtered by the chip its boot banner named),
/// else the spinner alone while it identifies (the Hardware row says
/// "identifying").
pub(crate) fn pending_primary(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
) -> Option<UiPrimary> {
    if let Some(flash) = offer_at(offers, board, "flash") {
        return Some(offer(install(flash).drawn(UiActionDraw::BoardPick {
            chip: pending.detected_chip.clone(),
            chip_from_banner: pending.detected_chip.is_some(),
        })));
    }
    (!pending.needs_firmware()).then(|| UiPrimary::working("Identifying"))
}

/// Install firmware: the `flash` offer, in the card's words (P12: install
/// what?).
fn install(flash: &UiOffer) -> UiCardAction {
    UiCardAction::press(flash, "Install firmware").with_icon("firmware")
}

fn offer(action: UiCardAction) -> UiPrimary {
    UiPrimary::Offer(action)
}

#[cfg(test)]
pub(crate) mod tests {
    use lpa_devices::view::{Escape, FirmwareFace};
    use lpa_devices::{ActivityKind, HoldLevel, HoldVia};

    use super::super::card_fixtures::{CardFixture, activity};
    use super::*;
    use crate::app::access::{UiDeviceAccess, UiUnlockOffer};
    use crate::app::devices::take_over_state::UiTakeOver;

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

    #[test]
    fn a_board_that_needs_firmware_installs_with_the_board_pick() {
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
        assert_eq!(
            action.draw,
            UiActionDraw::BoardPick {
                chip: Some("esp32c6".to_string()),
                chip_from_banner: true
            }
        );
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

    /// Offline with no way in: no button; the Link row says how long it
    /// has been away.
    #[test]
    fn an_offline_board_nothing_reaches_has_no_primary() {
        let mut fixture = CardFixture::offline().over(UiLinkKind::Bluetooth);
        fixture.view.escapes = vec![Escape::Forget];
        fixture.last_seen_at = Some(fixture.now - 15.0 * 86_400.0);
        assert_eq!(primary_action(&fixture.input(), None), None);
        assert_eq!(
            offline_words(fixture.last_seen_at, fixture.now),
            "Offline · 2 weeks"
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
    fn a_failed_connect_leaves_connect_to_press_again() {
        let mut fixture = CardFixture::ready();
        fixture.connection = BoardConnection::Failed {
            reason: "the board did not answer".to_string(),
        };
        let action = offered(&mut fixture);
        assert_eq!(
            (action.word.as_str(), path(&action)),
            ("Connect", "connect".to_string())
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
        assert_eq!(action.word, "Connect");
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

    /// Busy in the other tab: no button; the Link row says it.
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
        // one the card has nothing to press, and the Link row says who has
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
            held_by_tab: false,
            escapes: vec![Escape::Forget],
        }
    }
}
