//! The account's board list (plan `lp2025/2026-10-19-2305-account-board-list`,
//! M2), end to end through the real effects layer:
//!
//! - an account board offline everywhere shows in Offline boards on a
//!   browser that never met it: its name, when it was last seen, its
//!   firmware and project, its last picture dimmed, and no Connect;
//! - a board this browser remembers takes the record's facts where the
//!   record saw it more recently, and keeps its own where it did;
//! - a USB connect that finds the account's current key on the board
//!   reports it once per connection, with the board's own facts;
//! - Remove from your account is Lasting, offered on an account board while
//!   signed in, takes the row off at once, and a failed forget brings it
//!   back and says so; Forget reads "Forget on this browser";
//! - a full list says so on the board's card.
//!
//! Every verb is pressed by its offer path.

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::access::account_keys::tests::account;

use super::account_boards_tests::{home_card, sign_in};
use super::ble_drop_tests::{bench_over_bluetooth, bluetooth_board};
use super::relay_connect_tests::{BOARD, relay_light_player, unplug, with_relay};
use super::*;

#[test]
fn an_offline_record_for_a_board_never_met_here_shows_in_offline_boards() {
    let device = relay_light_player("dev00000acctlst01");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-list-1");
    let _rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    let seen = bench.clock.get() - 7_200.0;
    list_offline(&mut bench, &tasks, BOARD, "Porch", seen);

    let entry = crate::account_board_device(key());
    let roster = bench.controller.device_roster_view();
    let split = crate::split_roster(&roster);
    assert!(
        split.remembered.iter().any(|tile| tile.id == entry),
        "in Offline boards"
    );
    let sections = bench
        .controller
        .view()
        .home
        .expect("the home view")
        .sections;
    assert!(
        sections.offline.iter().any(|board| board.title == "Porch"),
        "{:?}",
        sections.offline
    );
    let card = home_card(&bench, entry);
    assert_eq!(card.presence, crate::UiBoardPresence::Offline);
    assert_eq!(card.surface.state, crate::SurfaceState::Offline);
    assert_eq!(
        card.surface
            .notice
            .as_ref()
            .and_then(|notice| notice.second.as_deref()),
        Some("It was last seen 2 hours ago.")
    );
    let cloud = bar(&card, crate::BarLayer::Cloud);
    assert_eq!(cloud.summary, "yona@example.com");
    assert_eq!(cloud.aside.as_deref(), Some("2 h ago"));
    assert!(cloud.stale, "last known: dimmed");
    let firmware = bar(&card, crate::BarLayer::Firmware);
    assert!(
        firmware.summary.contains("2026.10.01-1"),
        "{:?}",
        firmware.summary
    );
    let project = bar(&card, crate::BarLayer::Project);
    assert!(
        project.summary.contains("rocaille"),
        "{:?}",
        project.summary
    );

    // Nothing reaches it: no Connect, and Remove from your account is its
    // one verb.
    let relay = bench.device_verb(entry, "connect-relay");
    bench.not_offered(&relay);
    let tree = bench.controller.view().offers;
    let verbs: Vec<String> = tree
        .own_verbs_of(tree.device_prefix(entry).unwrap())
        .map(|offer| offer.path.to_string())
        .collect();
    assert_eq!(verbs, [format!("devices/mac-{BOARD}/forget-from-account")]);

    // Its last picture is asked for once, and drawn dimmed.
    assert_eq!(
        roster.cloud_picture_asks,
        [crate::CloudPictureAsk {
            id: BOARD.to_string(),
            seq: None
        }]
    );
    bench
        .controller
        .take_account_board_pictures(vec![crate::AccountBoardPicture {
            id: BOARD.to_string(),
            seq: 1_760_000_000_000,
            at: seen,
            outputs: vec![2],
            colors: Some(vec![255, 0, 0, 0, 0, 255]),
        }]);
    let card = home_card(&bench, entry);
    assert_eq!(card.picture.source, crate::PictureSource::Cloud);
    assert!(card.picture.dim, "last known");
    assert!(card.picture.frame.is_some());
    assert!(
        bench
            .controller
            .device_roster_view()
            .cloud_picture_asks
            .is_empty(),
        "a kept picture does not change while the board is away"
    );
}

/// This bench keeps no registry row, so the browser's own time for the
/// board is unknown and any record is newer (the comparison itself is
/// `account_boards_flow`'s `newer`, tested there).
#[test]
fn a_remembered_board_takes_the_records_newer_facts() {
    let device = relay_light_player("dev00000acctlst02");
    let (mut bench, tasks) = identified(&device, "usb-account-list-2");
    let _rig = with_relay(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    let target = bench.view().devices[0].id;
    unplug(&mut bench, &tasks, target);
    let seen = bench.clock.get() - 600.0;

    list_offline(&mut bench, &tasks, BOARD, "Porch", seen);
    let roster = bench.controller.device_roster_view();
    assert_eq!(roster.roster.devices.len(), 1, "no second card");
    let card = roster
        .roster
        .devices
        .iter()
        .find(|card| card.id == target)
        .unwrap();
    assert_eq!(card.title, "Porch");
    assert_eq!(card.remembered_firmware.as_deref(), Some("2026.10.01-1"));
    assert_eq!(roster.last_seen.get(&target), Some(&seen));
    assert!(
        crate::split_roster(&roster)
            .remembered
            .iter()
            .any(|tile| tile.id == target),
        "still an offline board"
    );
    let forget = bench.device_verb(target, "forget");
    let offer = bench.offered(&forget);
    assert_eq!(offer.label(), "Forget on this browser");
    assert!(
        offer.summary().contains("still shows from your account"),
        "{}",
        offer.summary()
    );
    let remove = bench.device_verb(target, "forget-from-account");
    bench.offered(&remove);
}

#[test]
fn a_usb_connect_that_finds_the_accounts_key_reports_the_board_once() {
    let device = relay_light_player("dev00000acctlst03");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-list-3");
    let requests = capture_requests(&mut bench);
    sign_in(&mut bench, &tasks);
    bench.granted.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    bench.run_until(&tasks, "the board's list to be read", |_| {
        !requests.borrow().is_empty()
    });
    for _ in 0..50 {
        bench.step(&tasks);
    }
    let sent = requests.borrow().clone();
    assert_eq!(sent.len(), 1, "once per connection: {sent:?}");
    let crate::AccountBoardRequest::Report(report) = &sent[0] else {
        panic!("not a report: {sent:?}");
    };
    assert_eq!(report.id, BOARD);
    assert_eq!(report.seen_on, crate::AccountBoardLink::Usb);
    assert_eq!(
        report.label, "lp-b49a",
        "no name provisioned: its LAN name, as its relay hello says it"
    );

    // Renamed here: the next report still says the board's own name.
    let target = bench.view().devices[0].id;
    let rename = bench.device_verb(target, "rename");
    bench
        .press(
            &rename,
            OfferArgs::new().with(crate::RENAME_NAME_PARAM, "Kitchen"),
        )
        .expect("renamed");
    bench.step(&tasks);
    assert_eq!(requests.borrow().len(), 1, "a rename reports nothing");
}

/// Over Bluetooth, the account's key unlocks the board at edit and its list
/// is read: the board is reported, as seen over Bluetooth.
#[test]
fn a_bluetooth_link_that_reads_the_accounts_key_reports_the_board() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev00000acctlst07")
            .with_untrusted_link()
            .with_root_files(vec![account_store_file()]),
    )));
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |bench| {
        bench
            .controller
            .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    });
    let requests = capture_requests(&mut bench);
    bench.run_until(&tasks, "the board to be reported", |_| {
        !requests.borrow().is_empty()
    });
    let sent = requests.borrow().clone();
    let crate::AccountBoardRequest::Report(report) = &sent[0] else {
        panic!("not a report: {sent:?}");
    };
    assert_eq!(report.id, "a0f26287b48c");
    assert_eq!(report.seen_on, crate::AccountBoardLink::Ble);
}

#[test]
fn signed_out_nothing_is_reported() {
    let device = relay_light_player("dev00000acctlst04");
    let (mut bench, tasks) = identified(&device, "usb-account-list-4");
    let requests = capture_requests(&mut bench);
    for _ in 0..50 {
        bench.step(&tasks);
    }
    assert!(requests.borrow().is_empty(), "nobody signed in");
}

#[test]
fn remove_from_your_account_is_lasting_takes_the_row_off_and_a_failure_brings_it_back() {
    let device = relay_light_player("dev00000acctlst05");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-account-list-5");
    let _rig = with_relay(&mut bench, &device);
    let requests = capture_requests(&mut bench);
    let entry = crate::account_board_device(key());
    let remove = format!("devices/mac-{BOARD}/forget-from-account");

    // Signed out: listed rows (a list that raced a sign-out) offer nothing.
    list_offline(&mut bench, &tasks, BOARD, "Porch", 1.0);
    bench.not_offered(&remove);

    sign_in(&mut bench, &tasks);
    list_offline(&mut bench, &tasks, BOARD, "Porch", 1.0);
    let offer = bench.offered(&remove);
    assert_eq!(offer.label(), "Remove from your account");
    assert!(
        offer.consequence().arms(),
        "Lasting: the agent never presses it"
    );
    assert!(
        offer
            .summary()
            .contains("comes back the next time it reaches lightplayer.app"),
        "{}",
        offer.summary()
    );

    bench
        .press_lasting(&remove, OfferArgs::new())
        .expect("armed and pressed");
    bench.step(&tasks);
    assert_eq!(
        requests.borrow().as_slice(),
        [crate::AccountBoardRequest::Forget {
            id: BOARD.to_string()
        }]
    );
    let gone = |bench: &DeviceBench| {
        !bench
            .controller
            .device_roster_view()
            .roster
            .devices
            .iter()
            .any(|card| card.id == entry)
    };
    assert!(gone(&bench), "off Offline boards at once");
    // A list read before the answer does not bring it back.
    list_offline(&mut bench, &tasks, BOARD, "Porch", 1.0);
    assert!(gone(&bench));

    bench
        .controller
        .account_board_answered(crate::AccountBoardAnswer::Failed {
            request: crate::AccountBoardRequest::Forget {
                id: BOARD.to_string(),
            },
            error: "lightplayer.app didn't answer.".to_string(),
        });
    bench.step(&tasks);
    assert!(!gone(&bench), "back, the forget failed");
    let card = home_card(&bench, entry);
    let cloud = bar(&card, crate::BarLayer::Cloud);
    assert!(
        cloud.details.sections.iter().any(|section| section
            .sentence
            .as_deref()
            .is_some_and(|sentence| sentence.contains("couldn't remove this board"))),
        "{cloud:?}"
    );
}

#[test]
fn a_full_list_says_so_on_the_boards_card() {
    let device = relay_light_player("dev00000acctlst06");
    let (mut bench, tasks) = identified(&device, "usb-account-list-6");
    sign_in(&mut bench, &tasks);
    let target = bench.view().devices[0].id;
    bench
        .controller
        .account_board_answered(crate::AccountBoardAnswer::ListFull {
            id: BOARD.to_string(),
            max: 64,
        });
    bench.step(&tasks);
    let card = home_card(&bench, target);
    let cloud = bar(&card, crate::BarLayer::Cloud);
    assert_eq!(cloud.aside.as_deref(), Some("list full"));
    let said: Vec<&str> = cloud
        .details
        .sections
        .iter()
        .filter_map(|section| section.sentence.as_deref())
        .filter(|sentence| sentence.contains("64 boards"))
        .collect();
    assert_eq!(
        said,
        ["Your account holds 64 boards. Remove one to add this one."],
        "said once"
    );
}

/// The account's list answers with one board, offline everywhere, last
/// seen `seen`.
fn list_offline(bench: &mut DeviceBench, tasks: &TaskPool, id: &str, label: &str, seen: f64) {
    bench
        .controller
        .set_account_boards(Some(crate::AccountBoardRows {
            at: bench.clock.get(),
            boards: vec![crate::AccountBoardRow {
                id: id.to_string(),
                label: label.to_string(),
                model: None,
                firmware: Some("2026.10.01-1".to_string()),
                project: Some("rocaille".to_string()),
                last_seen: Some(seen),
                online: None,
            }],
        }));
    bench.step(tasks);
}

/// A board's access store holding the account's key alone, nothing open.
fn account_store_file() -> (String, Vec<u8>) {
    let store = lpc_access::DeviceAccessFile {
        version: lpc_access::DeviceAccessFile::VERSION,
        secrets: vec![account(None).held_keys()[0].key.entry(1)],
        ble_enabled: true,
        open: lpc_access::OpenTo::Nobody,
    };
    (
        lpc_access::DeviceAccessFile::PATH.to_string(),
        store.to_json().expect("the store serializes").into_bytes(),
    )
}

/// Every request core hands the web edge, in order.
fn capture_requests(bench: &mut DeviceBench) -> Rc<RefCell<Vec<crate::AccountBoardRequest>>> {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&requests);
    bench
        .controller
        .set_on_account_board_request(move |request, _| sink.borrow_mut().push(request.clone()));
    requests
}

/// The card's row for `layer`.
fn bar(card: &crate::UiBoardCard, layer: crate::BarLayer) -> crate::UiStackBar {
    card.bars
        .iter()
        .find(|bar| bar.layer == layer)
        .cloned()
        .unwrap_or_else(|| panic!("no {layer:?} row"))
}

/// The relay board's MAC.
fn key() -> lpa_devices::BoardKey {
    lpa_devices::BoardKey::parse(BOARD).unwrap()
}
