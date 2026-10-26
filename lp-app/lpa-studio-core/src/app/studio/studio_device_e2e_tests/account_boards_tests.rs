//! Your boards on every computer (plan
//! `lp2025/2026-10-19-2300-your-boards-follow-you`, M1), end to end through
//! the real effects layer: the signed-in account's list (`ListAccountBoards`)
//! joined onto the roster by MAC.
//!
//! - a board this browser never met is a card in Online boards, its
//!   picture from lightplayer.app, Connect through lightplayer.app its
//!   primary; pressed, it dials `relay:<mac>`, and once its hello says who
//!   it is the in-memory entry folds into the board the roster remembers;
//! - a board with a USB link keeps it: the list only lets its
//!   lightplayer.app row say online, and nothing is offered through the
//!   relay;
//! - a remembered board, unplugged, moves to Online boards while the list
//!   names it, and back when it leaves the list;
//! - signed out, the list keeps nothing.
//!
//! Every verb is pressed by its offer path.

use crate::app::access::account_keys::tests::account;

use super::best_link_tests::list as list_on_this_network;
use super::relay_connect_tests::{BOARD, relay_light_player, unplug, with_relay};
use super::wifi_connect_tests::{KEY, joined_light_player, with_lan};
use super::*;

#[test]
fn a_board_never_met_here_is_online_and_connects_through_lightplayer_app() {
    let device = relay_light_player("dev00000acctbrd01");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-boards-1");
    let rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    assert!(bench.view().devices.is_empty(), "this browser met nothing");

    list(&mut bench, &tasks, &[BOARD]);
    let entry = crate::account_board_device(key());
    let roster = bench.controller.device_roster_view();
    let card = roster
        .roster
        .devices
        .iter()
        .find(|card| card.id == entry)
        .expect("the account's board is on the roster");
    assert_eq!(card.title, "Fabric Test");
    assert_eq!(card.state_label, "Online at lightplayer.app");
    let split = crate::split_roster(&roster);
    assert!(
        split.connected.iter().any(|card| card.id == entry),
        "in Online boards"
    );
    assert_eq!(
        roster.cloud_picture_asks,
        [crate::CloudPictureAsk {
            id: BOARD.to_string(),
            seq: None
        }],
        "the home page asks for its picture"
    );

    // Its picture arrives: the card draws it.
    bench
        .controller
        .take_account_board_pictures(vec![crate::AccountBoardPicture {
            id: BOARD.to_string(),
            seq: 3,
            at: 1.0,
            outputs: vec![2],
            colors: Some(vec![255, 0, 0, 0, 0, 255]),
        }]);
    let card = home_card(&bench, entry);
    assert_eq!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
    assert_eq!(card.picture.source, crate::PictureSource::Cloud);
    assert_eq!(card.presence, crate::UiBoardPresence::Online);
    assert_eq!(
        bench.controller.device_roster_view().cloud_picture_asks[0].seq,
        Some(3),
        "the next ask names the picture it holds"
    );

    let connect = bench.device_verb(entry, "connect-relay");
    assert_eq!(
        connect.to_string(),
        format!("devices/mac-{BOARD}/connect-relay"),
        "the board ref is its MAC"
    );
    let offer = bench.offered(&connect);
    assert!(offer.is_enabled());
    match &card.name_bar.primary {
        Some(crate::UiPrimary::Offer(primary)) => assert_eq!(primary.offer, connect),
        other => panic!("Connect is the primary, not {other:?}"),
    }
    // A board never met has no other verbs but the list's own: nothing
    // about it is known, and the account's list holds it.
    let tree = bench.controller.view().offers;
    let verbs: Vec<String> = tree
        .own_verbs_of(tree.device_prefix(entry).unwrap())
        .map(|offer| offer.path.to_string())
        .collect();
    assert_eq!(
        verbs,
        [
            connect.to_string(),
            format!("devices/mac-{BOARD}/forget-from-account")
        ]
    );

    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(
        &tasks,
        "the board to say hello through lightplayer.app",
        |bench| {
            bench
                .view()
                .devices
                .iter()
                .any(|card| card.activity.is_none() && card.state_label == "Ready")
        },
    );
    assert_eq!(rig.asked.borrow().as_slice(), [BOARD]);
    let roster = bench.controller.device_roster_view();
    assert_eq!(
        roster.roster.devices.len(),
        1,
        "one board, one card: the entry folded into it: {:?}",
        roster.roster.devices
    );
    let board = roster.roster.devices[0].id;
    assert_ne!(board, entry, "the roster's own board now");
    assert!(roster.cloud_boards.contains_key(&board));
    assert!(
        roster.cloud_picture_asks.is_empty(),
        "a board this tab reaches draws its own picture"
    );
    let verb = bench.device_verb(board, "connect-relay");
    bench.not_offered(&verb);
}

/// One press is enough (defect 2026-10-25, "a relay connect looked like it
/// needed two presses"): from the press until the board says hello, the
/// card the person pressed says it is connecting through lightplayer.app,
/// with its spinner — through the relay's answer and while the board is
/// identified, never its Connect again, and no second card beside it — and a
/// second press dials nothing.
#[test]
fn one_press_on_a_board_never_met_says_connecting_until_its_hello() {
    let device = relay_light_player("dev00000acctbrd09");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-boards-9");
    let rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, &[BOARD]);
    let entry = crate::account_board_device(key());
    let connect = bench.device_verb(entry, "connect-relay");
    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts");

    let mut turns = 0;
    while bench
        .controller
        .view()
        .home
        .expect("the home view")
        .devices
        .cards
        .iter()
        .any(|card| card.device == entry)
    {
        let cards = bench.controller.view().home.unwrap().devices.cards;
        assert_eq!(
            cards.len(),
            1,
            "turn {turns}: one card for the board while it connects: {:?}",
            cards.iter().map(|card| card.surface.state).collect::<Vec<_>>()
        );
        assert_connecting_through_lightplayer_app(&cards[0], turns);
        // A second press finds the verb waiting, and a double click that
        // races the disable dials nothing.
        let again = bench.offered(&connect);
        assert!(!again.is_enabled(), "turn {turns}: the verb waits");
        let _ = drive(bench.controller.dispatch(again.action.clone()));
        assert_eq!(rig.asked.borrow().len(), 1, "turn {turns}: one dial");
        bench.step(&tasks);
        turns += 1;
        assert!(turns < 200, "the board never said hello");
    }
    assert!(turns > 1, "the connect took more than the press's own turn");
    bench.run_until(&tasks, "the board ready on the relay", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    assert_eq!(rig.asked.borrow().as_slice(), [BOARD], "one dial in all");
    assert!(
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .is_empty(),
        "the hello ended the connect: nothing left to say"
    );
}

/// The card says a connect through lightplayer.app is under way, with its
/// spinner and no button to press again.
pub(super) fn assert_connecting_through_lightplayer_app(card: &crate::UiBoardCard, turn: usize) {
    assert_eq!(
        card.surface.state,
        crate::SurfaceState::Connecting,
        "turn {turn}: the card says it is connecting"
    );
    assert_eq!(
        card.surface.notice.as_ref().map(|notice| notice.first.as_str()),
        Some("Connecting to this board through lightplayer.app."),
        "turn {turn}"
    );
    assert_eq!(
        card.surface.progress,
        Some(crate::SurfaceProgress::Spinner),
        "turn {turn}"
    );
    assert!(
        !matches!(card.name_bar.primary, Some(crate::UiPrimary::Offer(_))),
        "turn {turn}: no button to press again: {:?}",
        card.name_bar.primary
    );
}

#[test]
fn a_board_with_a_usb_link_keeps_it_and_the_list_only_says_online() {
    let device = relay_light_player("dev00000acctbrd02");
    let (mut bench, tasks) = identified(&device, "usb-account-boards-2");
    let _rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, &[BOARD]);
    let target = bench.view().devices[0].id;
    let roster = bench.controller.device_roster_view();
    assert_eq!(roster.roster.devices.len(), 1, "no second card");
    assert!(roster.cloud_boards.contains_key(&target));
    assert!(roster.cloud_picture_asks.is_empty());
    let card = home_card(&bench, target);
    assert_ne!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
    assert_ne!(card.picture.source, crate::PictureSource::Cloud);
    let verb = bench.device_verb(target, "connect-relay");
    bench.not_offered(&verb);
}

#[test]
fn a_remembered_board_moves_to_online_boards_while_the_list_names_it() {
    let device = relay_light_player("dev00000acctbrd03");
    let (mut bench, tasks) = identified(&device, "usb-account-boards-3");
    let _rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    let target = bench.view().devices[0].id;
    unplug(&mut bench, &tasks, target);
    let remembered = |bench: &DeviceBench| {
        crate::split_roster(&bench.controller.device_roster_view())
            .remembered
            .iter()
            .any(|tile| tile.id == target)
    };
    assert!(remembered(&bench), "unplugged, it is an offline board");

    list(&mut bench, &tasks, &[BOARD]);
    assert!(!remembered(&bench), "listed, it is online");
    let card = home_card(&bench, target);
    assert_eq!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
    assert_eq!(card.presence, crate::UiBoardPresence::Online);
    let verb = bench.device_verb(target, "connect-relay");
    bench.offered(&verb);
    // What it plays is what it tells lightplayer.app now, not "not known"
    // until a link reaches it (Yona's new C6, 2026-10-25).
    let roster = bench.controller.device_roster_view();
    let tile = roster
        .roster
        .devices
        .iter()
        .find(|device| device.id == target)
        .expect("its card");
    assert_eq!(
        tile.loaded_project,
        lpa_devices::view::LoadedProject::Running {
            label: "rainbow".to_string()
        }
    );

    // It leaves the list: back to what this browser knew of it.
    list(&mut bench, &tasks, &[]);
    assert!(remembered(&bench));
    assert_eq!(
        home_card(&bench, target).presence,
        crate::UiBoardPresence::Offline
    );
}

/// The walk's board: met over USB with its Wi‑Fi joined (so this browser
/// remembers its address, and Connect takes the Wi‑Fi road first while the
/// list says it shares this network), then unplugged and listed.
/// lightplayer.app's row still offers its own way in.
#[test]
fn a_remembered_board_with_a_wifi_address_keeps_connect_relay_on_its_row() {
    let device = joined_light_player("dev00000acctbrd05");
    let (mut bench, tasks) = identified(&device, "usb-account-boards-5");
    let _lan = with_lan(&mut bench, &device);
    let _rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    let target = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    unplug(&mut bench, &tasks, target);
    list_on_this_network(&mut bench, &tasks, KEY, None, true);
    let relay = bench.device_verb(target, "connect-relay");
    bench.offered(&relay);
    let card = home_card(&bench, target);
    assert_eq!(card.surface.state, crate::SurfaceState::OnlineAtCloud);
    let cloud = card
        .bars
        .iter()
        .find(|bar| bar.layer == crate::BarLayer::Cloud)
        .unwrap();
    assert!(
        cloud
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .any(|verb| verb.offer == relay),
        "the lightplayer.app row offers it: {cloud:?}"
    );
}

#[test]
fn signed_out_the_list_keeps_nothing_and_nothing_is_offered() {
    let device = relay_light_player("dev00000acctbrd04");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-boards-4");
    let _rig = with_relay(&mut bench, &device);
    let entry = crate::account_board_device(key());

    // Rows with nobody signed in (a list that raced a sign-out): the card
    // may show, but nothing opens it through the relay.
    list(&mut bench, &tasks, &[BOARD]);
    let verb = bench.device_verb(entry, "connect-relay");
    bench.not_offered(&verb);

    sign_in(&mut bench, &tasks);
    let verb = bench.device_verb(entry, "connect-relay");
    bench.offered(&verb);

    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(None));
    bench.step(&tasks);
    let roster = bench.controller.device_roster_view();
    assert!(roster.roster.devices.is_empty(), "{:?}", roster.roster);
    assert!(roster.cloud_boards.is_empty());
}

/// Sign in: the account's keys reach core.
pub(super) fn sign_in(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    bench.step(tasks);
}

/// The account's list answers with `boards`, online.
pub(super) fn list(bench: &mut DeviceBench, tasks: &TaskPool, boards: &[&str]) {
    bench
        .controller
        .set_account_boards(Some(crate::AccountBoardRows {
            at: 100.0,
            boards: boards
                .iter()
                .map(|id| crate::AccountBoardRow {
                    id: id.to_string(),
                    label: "Fabric Test".to_string(),
                    model: None,
                    firmware: Some("2026.10.19-1".to_string()),
                    project: Some("rainbow".to_string()),
                    last_seen: Some(50.0),
                    online: Some(crate::AccountBoardPresence {
                        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
                        lan: None,
                        same_network: false,
                        since: 50.0,
                        relay_proto: 2,
                    }),
                })
                .collect(),
        }));
    bench.step(tasks);
}

/// The relay board's MAC.
fn key() -> lpa_devices::BoardKey {
    lpa_devices::BoardKey::parse(BOARD).unwrap()
}

/// The card the home view publishes for `device`.
pub(super) fn home_card(bench: &DeviceBench, device: crate::DeviceId) -> crate::UiBoardCard {
    bench
        .controller
        .view()
        .home
        .expect("the home view")
        .devices
        .cards
        .into_iter()
        .find(|card| card.device == device)
        .unwrap_or_else(|| panic!("no card for {device:?}"))
}

