//! Wi‑Fi settings over the bench's USB link, end to end: the network
//! controller's read on connect, the offers at `devices/<board>/wifi/…`
//! pressed by path, and the fake's REAL `LpServer` storing the network file
//! (Wi‑Fi roadmap M5, plan P4; the network list, P7).
//!
//! The password never leaves the form and the request: these tests look for
//! it in every string Studio can show or say — the view, the offer tree,
//! the app agent's readout, the console — and in what the agent hears.

use super::*;

/// A made-up network (plan: walks and tests never use a real one).
const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";
const SECOND_SSID: &str = "lp-back-office";
const SECOND_PASSWORD: &str = "staple-battery-7";

/// Connect over USB → the status is read once; add two networks through
/// the offer → the board holds them, the card lists them and the first
/// one's test shows "Saved" in its row (today's firmware); the switches
/// change one setting each; each network has its own Lasting Forget, which
/// leaves the other and the switches. Nothing Studio shows or logs holds a
/// password.
#[test]
fn wifi_is_read_on_connect_added_through_its_offer_and_forgotten() {
    let device = empty_light_player("dev000000wifi00001");
    let (mut bench, tasks) = identified(&device, "usb-wifi-1");
    let target = bench.view().devices[0].id;

    let status = wifi_status(&mut bench, &tasks, target);
    assert!(status.networks.is_empty(), "a fresh board has no network");
    assert!(status.wifi, "Wi‑Fi is on by default");
    assert!(status.cloud_relay, "the relay is on by default");
    assert_eq!(status.station, crate::StationState::Unsupported);
    let add = wifi_verb(&mut bench, target, "add");
    assert!(bench.offered(&add).takes_a_secret());
    assert_eq!(bench.offered(&add).label(), "Save");
    let wifi = bench.controller.device_roster_view().wifi[&target].clone();
    assert!(
        wifi.nothing_saved(),
        "the popover opens on the connect page"
    );

    for (ssid, password) in [(SSID, PASSWORD), (SECOND_SSID, SECOND_PASSWORD)] {
        bench
            .press(
                &add,
                OfferArgs::new()
                    .with(crate::WIFI_NETWORK_PARAM, ssid)
                    .with(crate::WIFI_PASSWORD_PARAM, password),
            )
            .expect("the add starts");
        wifi_status(&mut bench, &tasks, target);
    }
    let status = wifi_status(&mut bench, &tasks, target);
    let names: Vec<&str> = status.networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, [SSID, SECOND_SSID]);
    assert!(status.networks.iter().all(|network| network.has_password));
    let wifi = bench.controller.device_roster_view().wifi[&target].clone();
    assert_eq!(wifi.row_value(), "2 saved");
    assert!(wifi.rows().iter().all(|row| row.word == "Saved"));
    let test = wifi
        .test()
        .expect("the last network added tests in its row");
    assert_eq!(test.ssid, SECOND_SSID);
    assert_eq!(test.result().unwrap().headline, "Saved");
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::DismissTest { device: target });
    assert_eq!(
        bench.controller.device_roster_view().wifi[&target].test(),
        None,
        "Done dismisses it"
    );
    assert_no_password_anywhere(&mut bench);

    // The board holds them: asked again (the popover's refresh), it answers
    // the same networks — read off its own file, not Studio's memory.
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::Refresh { device: target });
    assert_eq!(
        wifi_status(&mut bench, &tasks, target).networks,
        status.networks
    );
    // A scan on a board that cannot scan asks nothing.
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::Scan { device: target });
    assert!(!bench.controller.device_roster_view().wifi[&target].scanning);

    // One switch at a time.
    let cloud_relay = wifi_verb(&mut bench, target, "cloud-relay");
    bench
        .press(
            cloud_relay,
            OfferArgs::new().with(crate::WIFI_ENABLED_PARAM, "false"),
        )
        .expect("the switch starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert!(!status.cloud_relay);
    assert!(status.wifi);
    let enabled = wifi_verb(&mut bench, target, "enabled");
    bench
        .press(
            enabled,
            OfferArgs::new().with(crate::WIFI_ENABLED_PARAM, "false"),
        )
        .expect("the switch starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert!(!status.wifi);
    assert_eq!(status.networks.len(), 2, "a switch keeps the networks");

    // Forget one: Lasting (the board forgets a password Studio never kept).
    let forget = wifi_verb(&mut bench, target, "forget").child("lp-walk-net");
    bench
        .press_lasting(forget, OfferArgs::new())
        .expect("the forget starts");
    let status = wifi_status(&mut bench, &tasks, target);
    let names: Vec<&str> = status.networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, [SECOND_SSID], "the other one stays");
    assert!(!status.cloud_relay, "the switches outlive the network");
    assert_no_password_anywhere(&mut bench);
}

/// The connect page asks for a scan when it opens, which is often while the
/// board's status read is still out. That scan is owed, not dropped: it goes
/// out once the read lands, and the Nearby list fills (PR C's emulated walk
/// found it staying empty, now and then). The board's station is scripted:
/// not connected, and its radio has heard one network.
#[test]
fn a_scan_asked_while_the_status_read_is_out_goes_after_it() {
    let device = wifi_light_player("dev000000wifiscan1");
    let (mut bench, tasks) = identified(&device, "usb-wifi-scan");
    let target = bench.view().devices[0].id;
    let status = wifi_status(&mut bench, &tasks, target);
    assert_eq!(status.station, crate::StationState::NotConnected);

    // A read goes out (the popover's refresh), and the scan is asked before
    // its answer has come back.
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::Refresh { device: target });
    assert!(
        bench.controller.device_roster_view().wifi[&target].reading,
        "the read is out"
    );
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::Scan { device: target });
    bench.run_until(&tasks, "the board's heard networks", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi
            .get(&target)
            .is_some_and(|wifi| wifi.heard.is_some())
    });
    let heard = bench.controller.device_roster_view().wifi[&target]
        .heard
        .clone()
        .unwrap();
    let names: Vec<&str> = heard.iter().map(|network| network.ssid.as_str()).collect();
    assert_eq!(names, [SSID]);
}

/// A password the board would refuse is refused by the offer first, in the
/// board's own words, without quoting it; nothing reaches the board.
#[test]
fn a_short_password_is_refused_before_the_board_hears_it() {
    let device = empty_light_player("dev000000wifi00002");
    let (mut bench, tasks) = identified(&device, "usb-wifi-2");
    let target = bench.view().devices[0].id;
    wifi_status(&mut bench, &tasks, target);

    // The offer refuses a short password before the board hears of it, and
    // never quotes it.
    let add = wifi_verb(&mut bench, target, "add");
    let refusal = bench
        .offered(&add)
        .press(
            &OfferArgs::new()
                .with(crate::WIFI_NETWORK_PARAM, SSID)
                .with(crate::WIFI_PASSWORD_PARAM, "short"),
        )
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("5 characters"), "{refusal}");
    assert!(!refusal.contains("short"), "{refusal}");
    assert!(wifi_status(&mut bench, &tasks, target).networks.is_empty());
}

/// The app agent never handles a password: a value for the secret is
/// refused, a press without one becomes the user's card — pre-filled with
/// the network name — and when the user presses it, the agent hears `•••`.
#[test]
fn the_agent_hands_wifi_to_the_user_and_never_hears_the_password() {
    let device = empty_light_player("dev000000wifi00003");
    let (mut bench, tasks) = identified(&device, "usb-wifi-3");
    let target = bench.view().devices[0].id;
    wifi_status(&mut bench, &tasks, target);
    let set = wifi_verb(&mut bench, target, "add").to_string();

    let readout = bench.controller.app_agent_readout_for_test().render();
    assert!(
        readout.contains("password (secret — the user types it)"),
        "{readout}"
    );

    let refused = act(
        &mut bench,
        &set,
        &[("network", SSID), ("password", PASSWORD)],
    );
    assert!(
        matches!(&refused, lpa_agent::ActOutcome::Refused { reason, .. }
            if reason.contains("never handles passwords") && !reason.contains(PASSWORD)),
        "{refused:?}"
    );
    assert!(app_cards(&mut bench).is_empty(), "nothing was handed over");

    let carded = act(&mut bench, &set, &[("network", SSID)]);
    assert!(
        matches!(carded, lpa_agent::ActOutcome::NeedsUser { .. }),
        "an offer that takes a secret is always the user's card: {carded:?}"
    );
    let card = app_cards(&mut bench).remove(0);
    assert_eq!(card.args.get(crate::WIFI_NETWORK_PARAM), Some(SSID));
    assert_eq!(card.args.get(crate::WIFI_PASSWORD_PARAM), None);
    assert!(
        wifi_status(&mut bench, &tasks, target).networks.is_empty(),
        "nothing was pressed"
    );

    // The user types the password on the card's form and presses it.
    bench
        .press(
            &set,
            OfferArgs::new()
                .with(crate::WIFI_NETWORK_PARAM, SSID)
                .with(crate::WIFI_PASSWORD_PARAM, PASSWORD),
        )
        .expect("the set starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert!(status.networks.iter().any(|network| network.has_password));
    let card = app_cards(&mut bench).remove(0);
    let heard = card.resume_text();
    assert!(heard.contains(crate::SECRET_MARKER), "{heard}");
    assert!(!heard.contains(PASSWORD), "{heard}");
    assert_eq!(
        card.user_args
            .as_ref()
            .and_then(|args| args.get(crate::WIFI_PASSWORD_PARAM)),
        Some(crate::SECRET_MARKER)
    );
    assert_no_password_anywhere(&mut bench);
}

/// Every string Studio can show or say: the whole view (the offer tree, the
/// cards, the agent's transcript), the console, the app agent's readout.
fn assert_no_password_anywhere(bench: &mut DeviceBench) {
    let view = format!("{:?}", bench.controller.view());
    assert!(!view.contains(PASSWORD), "the view holds the password");
    let logs = format!("{:?}", bench.controller.logs());
    let readout = bench.controller.app_agent_readout_for_test().render();
    for password in [PASSWORD, SECOND_PASSWORD] {
        assert!(!view.contains(password), "the view holds a password");
        assert!(!logs.contains(password), "the console holds a password");
        assert!(!readout.contains(password), "the readout holds a password");
    }
}

/// A LightPlayer board with a Wi‑Fi station: not connected, and its radio
/// has heard [`SSID`] (the firmware's station probes, scripted).
fn wifi_light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(
        FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new()
                .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
                .with_base_mac("60:55:f9:0a:0b:0d")
                .with_heartbeat_interval(Duration::from_millis(20)),
        ))
        .with_wifi_station(lpa_link::providers::fake_device::FakeWifiStation {
            state: || lpc_wire::StationState::NotConnected,
            scan: || {
                lpc_wire::NetworkScan::Heard(vec![lpc_wire::server::HeardNetwork {
                    ssid: SSID.to_string(),
                    rssi: -50,
                    secure: true,
                }])
            },
        }),
    )
}

/// Read the board's status as the card has it, once it has answered and
/// nothing is in flight.
fn wifi_status(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    device: crate::DeviceId,
) -> crate::NetworkStatus {
    bench.run_until(tasks, "the board's Wi‑Fi status", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi
            .get(&device)
            .is_some_and(|wifi| wifi.status.is_some() && !wifi.reading && !wifi.writing)
    });
    bench.controller.device_roster_view().wifi[&device]
        .status
        .clone()
        .unwrap()
}

/// Where `device`'s Wi‑Fi verb `verb` lives: `devices/<board>/wifi/<verb>`.
fn wifi_verb(bench: &mut DeviceBench, device: crate::DeviceId, verb: &str) -> crate::OfferPath {
    bench.device_verb(device, "wifi").child(verb)
}

/// The radio rows hush from their own details (the setup rows' P3): Disable
/// Wi‑Fi under the network list, then Enable Wi‑Fi once it is off; Disable
/// lightplayer.app, then Enable — each the board's own switch, pressed by
/// the offer path and args the card carries, and the row says "disabled"
/// and wants nothing while it is off.
#[test]
fn the_radio_rows_hush_from_their_own_details() {
    let device = wifi_light_player("dev000000wifi00009");
    let (mut bench, tasks) = identified(&device, "usb-wifi-9");
    let target = bench.view().devices[0].id;
    wifi_status(&mut bench, &tasks, target);

    let card = home_card(&bench, target);
    let wifi = card.bar(crate::BarLayer::Wifi);
    assert_eq!(wifi.summary, "not connected");
    assert_eq!(wifi.opens_details, Some("Set up"));
    assert!(wifi.todo, "nothing saved: a new board's to-do");
    let disable = verb_of(&wifi.details.foot, "Disable Wi\u{2011}Fi");
    bench
        .press(&disable.offer, disable.args.clone())
        .expect("Disable Wi‑Fi presses the board's switch");
    assert!(!wifi_status(&mut bench, &tasks, target).wifi);
    let card = home_card(&bench, target);
    let wifi = card.bar(crate::BarLayer::Wifi);
    assert_eq!(wifi.summary, "disabled");
    assert!(!wifi.todo, "disabled wants nothing");
    assert_eq!(wifi.opens_details, None);
    let enable = verb_of(&wifi.details.sections, "Enable Wi\u{2011}Fi");
    bench
        .press(&enable.offer, enable.args.clone())
        .expect("Enable Wi‑Fi presses it back");
    assert!(wifi_status(&mut bench, &tasks, target).wifi);

    let card = home_card(&bench, target);
    let cloud = card.bar(crate::BarLayer::Cloud);
    let disable = verb_of(&cloud.details.sections, "Disable lightplayer.app");
    bench
        .press(&disable.offer, disable.args.clone())
        .expect("Disable lightplayer.app presses Cloud relay");
    assert!(!wifi_status(&mut bench, &tasks, target).cloud_relay);
    let card = home_card(&bench, target);
    let cloud = card.bar(crate::BarLayer::Cloud);
    assert_eq!(cloud.summary, "disabled");
    assert!(!cloud.todo);
    let enable = verb_of(&cloud.details.sections, "Enable lightplayer.app");
    bench
        .press(&enable.offer, enable.args.clone())
        .expect("Enable lightplayer.app presses it back");
    assert!(wifi_status(&mut bench, &tasks, target).cloud_relay);
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

/// The verb worded `word` among `sections`' affordances.
#[track_caller]
fn verb_of(
    sections: &[crate::RichSection<crate::UiCardAction>],
    word: &str,
) -> crate::UiCardAction {
    sections
        .iter()
        .flat_map(|section| section.affordances.iter())
        .find(|action| action.word == word)
        .cloned()
        .unwrap_or_else(|| panic!("no {word:?} in {sections:?}"))
}

/// Bluetooth on and off is a core offer (`devices/<board>/bluetooth/enabled`):
/// the Bluetooth row's Disable presses it by path over USB, the board's
/// access file says off once it is back (Studio restarts it to apply), and
/// the row says "disabled" with Enable in its details.
#[test]
fn disable_bluetooth_is_a_core_offer_pressed_from_its_row() {
    let device = wifi_light_player("dev000000wifi00010");
    let (mut bench, tasks) = identified(&device, "usb-wifi-10");
    let target = bench.view().devices[0].id;
    bench.run_until(&tasks, "the board's access list", |bench| {
        bench
            .controller
            .device_roster_view()
            .access
            .get(&target)
            .and_then(|access| access.panel.as_ref())
            .is_some_and(|panel| panel.ble_enabled == Some(true))
    });
    let card = home_card(&bench, target);
    let bluetooth = card.bar(crate::BarLayer::Bluetooth);
    assert_eq!(bluetooth.summary, "LP-0b0d");
    let disable = verb_of(&bluetooth.details.sections, crate::DISABLE_BLUETOOTH);
    assert_eq!(
        disable.offer,
        bench.device_verb(target, "bluetooth").child("enabled")
    );
    assert_eq!(
        *bench.offered(&disable.offer).consequence(),
        crate::ActionConsequence::Undoable,
        "it restarts the board: the error tint, one click"
    );
    bench
        .press(&disable.offer, disable.args.clone())
        .expect("Disable Bluetooth presses the core offer");
    bench.run_until(&tasks, "the board to say Bluetooth is off", |bench| {
        bench
            .controller
            .device_roster_view()
            .access
            .get(&target)
            .and_then(|access| access.panel.as_ref())
            .is_some_and(|panel| panel.ble_enabled == Some(false) && !panel.restart_pending)
    });
    let card = home_card(&bench, target);
    let bluetooth = card.bar(crate::BarLayer::Bluetooth);
    assert_eq!(bluetooth.summary, "disabled");
    verb_of(&bluetooth.details.sections, crate::ENABLE_BLUETOOTH);
}
