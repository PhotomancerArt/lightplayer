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
