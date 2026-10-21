//! Connect takes the best road, in one press (`best_link.rs`), end to end
//! through the real effects layer:
//!
//! - a board the account's list says shares this network is reached over
//!   Wi‑Fi first, at the address the list gave, even one this browser never
//!   met; the list saying another network, or a page that cannot reach the
//!   LAN, takes lightplayer.app first;
//! - a Wi‑Fi try that fails goes on through lightplayer.app in the same
//!   press, with one line in the log; when both fail, the card says
//!   lightplayer.app's words and lets go of its Connect;
//! - "Connect over Wi‑Fi" and "Connect through lightplayer.app" pressed
//!   alone are that road alone;
//! - a board this tab already reaches has no road, and the list's address is
//!   never dialled or remembered;
//! - watching a board on its card opens nothing.
//!
//! Every verb is pressed by its offer path, as the card and the app agent
//! press it.

use crate::app::access::account_keys::tests::account;

use super::connected_tests::tick_until;
use super::relay_connect_tests::{BOARD, relay_light_player, unplug, with_relay};
use super::wifi_connect_tests::{KEY, joined_light_player, with_lan};
use super::*;

/// The address the account's list says the board gave on its network.
const ROW_LAN: &str = "192.168.4.20:80";
/// The socket Studio dials for it.
const ROW_URL: &str = "ws://192.168.4.20:80/link";

/// How a socket says each way a Wi‑Fi try fails.
const LAN_BLOCKED: &str = "wi-fi connect to ws://192.168.4.20:80/link failed: \
                           net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS";
const LAN_TIMEOUT: &str = "wi-fi connect timed out after 10 s";
const LAN_BUSY: &str = "wi-fi link lost: the board closed the link (code 1013: try again later)";
/// How the relay says the board is not connected to it.
const RELAY_OFFLINE: &str = "relay link lost: the board closed the link (code 4404: board-offline)";

#[test]
fn a_board_never_met_that_shares_this_network_is_reached_over_wifi_first() {
    let (mut bench, tasks, lan, relay, entry) = never_met(ROW_LAN, true);

    let wifi = bench.device_verb(entry, "connect-wifi");
    let offer = bench.offered(&wifi);
    assert!(offer.summary().contains(ROW_LAN), "{}", offer.summary());
    let primary = primary(&bench, entry);
    assert_eq!(primary.offer, wifi, "Wi‑Fi is the first road");
    assert_eq!(primary.icon.as_deref(), Some("wifi"));
    let relay_verb = bench.device_verb(entry, "connect-relay");
    bench.offered(&relay_verb);

    bench
        .press(&wifi, OfferArgs::new())
        .expect("the connect starts");
    wait_ready(&mut bench, &tasks);
    assert_eq!(lan.asked.borrow().as_slice(), [ROW_URL]);
    assert!(relay.asked.borrow().is_empty(), "lightplayer.app untouched");
    let board = bench.view().devices[0].id;
    let line = &bench.controller.device_roster_view().lan_links[&board];
    assert_eq!(line.kind, crate::UiLinkKind::Wifi, "reached on Wi‑Fi");
    // The list's address is used, not remembered.
    assert!(lan.stored.borrow().is_empty(), "{:?}", lan.stored.borrow());
    assert!(
        bench
            .controller
            .wifi_addresses()
            .get(&board_key())
            .is_none()
    );
}

#[test]
fn a_board_on_another_network_is_reached_through_lightplayer_app_and_wifi_is_never_asked() {
    let (mut bench, tasks, lan, relay, entry) = never_met(ROW_LAN, false);
    let wifi = bench.device_verb(entry, "connect-wifi");
    bench.not_offered(&wifi);
    let relay_verb = bench.device_verb(entry, "connect-relay");
    let primary = primary(&bench, entry);
    assert_eq!(primary.offer, relay_verb);
    assert_eq!(primary.icon.as_deref(), Some("cloud"));

    bench
        .press(&relay_verb, OfferArgs::new())
        .expect("the connect starts");
    wait_ready(&mut bench, &tasks);
    assert_eq!(relay.asked.borrow().as_slice(), [BOARD]);
    assert!(lan.asked.borrow().is_empty(), "the LAN never asked");
}

#[test]
fn a_page_that_cannot_reach_the_lan_goes_through_lightplayer_app_and_one_after_the_prompt_tries_wifi()
 {
    let (mut bench, tasks, _lan, _relay, entry) = never_met(ROW_LAN, true);
    bench.controller.set_lan_reach(crate::LanReach::Blocked);
    bench.step(&tasks);
    let wifi = bench.device_verb(entry, "connect-wifi");
    bench.not_offered(&wifi);
    assert_eq!(
        primary(&bench, entry).offer,
        bench.device_verb(entry, "connect-relay")
    );

    bench.controller.set_lan_reach(crate::LanReach::AfterPrompt);
    bench.step(&tasks);
    assert_eq!(
        primary(&bench, entry).offer,
        bench.device_verb(entry, "connect-wifi")
    );
}

#[test]
fn a_remembered_board_takes_the_lists_word_for_which_road_leads() {
    let (mut bench, tasks, _lan, _relay, target) = remembered_offline();
    let connect = bench.device_verb(target, "connect");
    // Signed out, no list: Wi‑Fi at the remembered address, as before.
    assert_eq!(reach(&mut bench, &connect), crate::ConnectReach::Wifi);

    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, KEY, Some(ROW_LAN), false);
    assert_eq!(
        reach(&mut bench, &connect),
        crate::ConnectReach::Relay,
        "online on another network: lightplayer.app first"
    );
    // "Connect over Wi‑Fi" by hand still dials the remembered address.
    let wifi = bench.device_verb(target, "connect-wifi");
    assert!(bench.offered(&wifi).summary().contains("192.168.4.100"));

    list(&mut bench, &tasks, KEY, None, true);
    assert_eq!(reach(&mut bench, &connect), crate::ConnectReach::Wifi);
    assert!(
        bench.offered(&wifi).summary().contains("192.168.4.100"),
        "the list gave no address: the remembered one"
    );

    list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);
    assert!(
        bench.offered(&wifi).summary().contains(ROW_LAN),
        "the list's address while it shares this network"
    );
}

#[test]
fn connect_dials_the_lists_address_for_a_remembered_board() {
    let (mut bench, tasks, lan, relay, target) = remembered_offline();
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);
    let connect = bench.device_verb(target, "connect");
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    tick_until(&mut bench, &tasks, "the board connected", |bench| {
        bench.controller.connected().is_some()
    });
    assert_eq!(lan.asked.borrow().as_slice(), [ROW_URL]);
    assert!(relay.asked.borrow().is_empty());
    assert!(
        bench
            .controller
            .wifi_addresses()
            .get(&lpa_devices::BoardKey::parse(KEY).unwrap())
            .is_some_and(|address| address.ip == "192.168.4.100"),
        "the list's address is never written to the book"
    );
}

#[test]
fn a_wifi_try_that_fails_goes_on_through_lightplayer_app_in_the_same_press() {
    for (what, raw) in [
        ("blocked", LAN_BLOCKED),
        ("timed out", LAN_TIMEOUT),
        ("busy", LAN_BUSY),
    ] {
        let (mut bench, tasks, lan, relay, target) = remembered_offline();
        sign_in(&mut bench, &tasks);
        list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);
        *lan.refuse.borrow_mut() = Some(raw.to_string());
        let connect = bench.device_verb(target, "connect");
        assert_eq!(reach(&mut bench, &connect), crate::ConnectReach::Wifi);
        bench.press(&connect, OfferArgs::new()).expect("Connect");
        tick_until(&mut bench, &tasks, "the board connected", |bench| {
            bench.controller.connected().is_some()
        });
        assert_eq!(lan.asked.borrow().as_slice(), [ROW_URL], "{what}");
        assert_eq!(relay.asked.borrow().as_slice(), [KEY], "{what}");
        let line = &bench.controller.device_roster_view().lan_links[&target];
        assert_eq!(line.kind, crate::UiLinkKind::Relay, "{what}");
        assert!(
            bench
                .controller
                .device_roster_view()
                .wifi_connects
                .is_empty(),
            "{what}: the Wi‑Fi failure is not shown once lightplayer.app answered"
        );
        let said: Vec<String> = bench
            .controller
            .logs()
            .iter()
            .map(|entry| entry.message.clone())
            .filter(|message| message.contains("trying lightplayer.app"))
            .collect();
        let name = &bench.view().devices[0].title;
        assert_eq!(
            said,
            [format!(
                "Couldn't reach {name} over Wi\u{2011}Fi; trying lightplayer.app."
            )],
            "{what}"
        );
    }
}

#[test]
fn a_board_never_met_falls_through_from_its_wifi_connect() {
    let (mut bench, tasks, lan, relay, entry) = never_met(ROW_LAN, true);
    *lan.refuse.borrow_mut() = Some(LAN_TIMEOUT.to_string());
    let primary = primary(&bench, entry);
    bench
        .press(&primary.offer, OfferArgs::new())
        .expect("the connect starts");
    wait_ready(&mut bench, &tasks);
    assert_eq!(lan.asked.borrow().as_slice(), [ROW_URL]);
    assert_eq!(relay.asked.borrow().as_slice(), [BOARD]);
    let board = bench.view().devices[0].id;
    assert_eq!(
        bench.controller.device_roster_view().lan_links[&board].kind,
        crate::UiLinkKind::Relay
    );
}

#[test]
fn when_both_roads_fail_the_card_says_lightplayer_apps_words_and_lets_go() {
    let (mut bench, tasks, lan, relay, target) = remembered_offline();
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);
    *lan.refuse.borrow_mut() = Some(LAN_TIMEOUT.to_string());
    *relay.refuse.borrow_mut() = Some(RELAY_OFFLINE.to_string());
    let connect = bench.device_verb(target, "connect");
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    tick_until(&mut bench, &tasks, "both roads to answer", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&target)
            .is_some_and(|connect| connect.error.is_some())
    });
    let said = &bench.controller.device_roster_view().wifi_connects[&target];
    assert!(said.through_relay);
    assert_eq!(said.error.as_deref(), Some(crate::RELAY_OFFLINE_WORDS));
    assert!(
        bench.controller.pending_lens().is_none(),
        "the held Connect let go: the card says why, not Connecting…"
    );
    assert!(bench.controller.connected().is_none());
}

#[test]
fn each_road_pressed_alone_is_that_road_alone() {
    let (mut bench, tasks, lan, relay, target) = remembered_offline();
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);

    *lan.refuse.borrow_mut() = Some(LAN_TIMEOUT.to_string());
    let wifi = bench.device_verb(target, "connect-wifi");
    bench
        .press(&wifi, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(&tasks, "the Wi‑Fi try to fail", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&target)
            .is_some_and(|connect| connect.error.is_some())
    });
    assert!(relay.asked.borrow().is_empty(), "no fall-through");
    let said = &bench.controller.device_roster_view().wifi_connects[&target];
    assert!(!said.through_relay, "the card keeps the Wi‑Fi words");

    *relay.refuse.borrow_mut() = Some(RELAY_OFFLINE.to_string());
    let relay_verb = bench.device_verb(target, "connect-relay");
    bench
        .press(&relay_verb, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(&tasks, "the relay try to fail", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&target)
            .is_some_and(|connect| connect.through_relay && connect.error.is_some())
    });
    assert_eq!(
        lan.asked.borrow().as_slice(),
        [ROW_URL],
        "no second Wi‑Fi try"
    );
}

#[test]
fn a_board_with_a_usb_link_has_no_road_and_the_lists_address_is_never_dialled() {
    let device = joined_light_player("dev00000bestlnk05");
    let (mut bench, tasks) = identified(&device, "usb-best-link-5");
    let lan = with_lan(&mut bench, &device);
    let relay = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, KEY, Some(ROW_LAN), true);
    let target = bench.view().devices[0].id;
    for verb in ["connect-wifi", "connect-relay"] {
        let verb = bench.device_verb(target, verb);
        bench.not_offered(&verb);
    }
    let connect = bench.device_verb(target, "connect");
    let tree = bench.controller.view().offers;
    let offer = tree.get(&connect).expect("Connect is on the card");
    assert_eq!(
        offer.action.op_as::<crate::RuntimeOp>(),
        Some(&crate::RuntimeOp::ConnectDevice {
            device: target,
            reach: None
        }),
        "no road: the session opens on the link it has"
    );
    for _ in 0..100 {
        bench.step(&tasks);
    }
    assert!(lan.asked.borrow().is_empty());
    assert!(relay.asked.borrow().is_empty());
    assert!(
        !bench
            .controller
            .device_roster_view()
            .lan_links
            .contains_key(&target),
        "still on its cable"
    );
}

/// Watching steps aside: a card drawn from the account's list opens no
/// session on either road while nobody presses.
#[test]
fn watching_a_board_on_its_card_opens_nothing() {
    let (mut bench, tasks, lan, relay, entry) = never_met(ROW_LAN, true);
    for _ in 0..200 {
        bench.step(&tasks);
        let _ = bench.controller.view();
    }
    assert_eq!(
        home_card(&bench, entry).surface.state,
        crate::SurfaceState::OnlineAtCloud
    );
    assert!(lan.asked.borrow().is_empty(), "{:?}", lan.asked.borrow());
    assert!(
        relay.asked.borrow().is_empty(),
        "{:?}",
        relay.asked.borrow()
    );
}

// --- helpers -------------------------------------------------------------

type Rigs = (
    DeviceBench,
    TaskPool,
    super::wifi_connect_tests::LanRig,
    super::relay_connect_tests::RelayRig,
    crate::DeviceId,
);

/// A board this browser never met, listed online with `lan` and
/// `same_network`, on a bench with both network halves and someone signed
/// in: its entry's handle.
fn never_met(lan: &str, same_network: bool) -> Rigs {
    let device = relay_light_player("dev00000bestlnk01");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-best-link-never-met");
    let lan_rig = with_lan(&mut bench, &device);
    let relay_rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, BOARD, Some(lan), same_network);
    let entry = crate::account_board_device(board_key());
    (bench, tasks, lan_rig, relay_rig, entry)
}

/// A board met over USB with its Wi‑Fi joined (so this browser remembers
/// its address), then unplugged: its handle.
fn remembered_offline() -> Rigs {
    let device = joined_light_player("dev00000bestlnk02");
    let (mut bench, tasks) = identified(&device, "usb-best-link-remembered");
    let lan = with_lan(&mut bench, &device);
    let relay = with_relay(&mut bench, &device);
    let target = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    bench.settle_library();
    unplug(&mut bench, &tasks, target);
    (bench, tasks, lan, relay, target)
}

/// Sign in: the account's keys reach core.
fn sign_in(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    bench.step(tasks);
}

/// The account's list answers with one board, `id`, online with `lan` and
/// `same_network`.
pub(super) fn list(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    id: &str,
    lan: Option<&str>,
    same_network: bool,
) {
    bench
        .controller
        .set_account_boards(Some(crate::AccountBoardRows {
            at: 100.0,
            boards: vec![crate::AccountBoardRow {
                id: id.to_string(),
                label: "Fabric Test".to_string(),
                model: None,
                firmware: Some("2026.10.19-1".to_string()),
                project: Some("rainbow".to_string()),
                last_seen: Some(50.0),
                online: Some(crate::AccountBoardPresence {
                    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
                    lan: lan.map(str::to_string),
                    same_network,
                    since: 50.0,
                    relay_proto: 2,
                }),
            }],
        }));
    bench.step(tasks);
}

/// The road `connect` takes now.
fn reach(bench: &mut DeviceBench, connect: &crate::OfferPath) -> crate::ConnectReach {
    match bench.offered(connect).action.op_as::<crate::RuntimeOp>() {
        Some(crate::RuntimeOp::ConnectDevice {
            reach: Some(reach), ..
        }) => *reach,
        other => panic!("not an offline board's Connect: {other:?}"),
    }
}

/// Wait for the one board to say hello and stand ready.
fn wait_ready(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench.run_until(tasks, "the board to say hello", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.activity.is_none() && card.state_label == "Ready")
    });
}

/// The never-met relay board's MAC.
fn board_key() -> lpa_devices::BoardKey {
    lpa_devices::BoardKey::parse(BOARD).unwrap()
}

/// The card's primary, as an offer.
fn primary(bench: &DeviceBench, device: crate::DeviceId) -> crate::UiCardAction {
    match home_card(bench, device).name_bar.primary {
        Some(crate::UiPrimary::Offer(primary)) => primary,
        other => panic!("no offer as the primary: {other:?}"),
    }
}

/// The card the home view publishes for `device`.
fn home_card(bench: &DeviceBench, device: crate::DeviceId) -> crate::UiBoardCard {
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
