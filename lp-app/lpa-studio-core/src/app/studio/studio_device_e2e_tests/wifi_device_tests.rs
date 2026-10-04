//! Wi‑Fi settings over the bench's USB link, end to end: the network
//! controller's read on connect, the offers at `devices/<board>/wifi/…`
//! pressed by path, and the fake's REAL `LpServer` storing the network file
//! (Wi‑Fi roadmap M5, plan P4).
//!
//! The password never leaves the form and the request: these tests look for
//! it in every string Studio can show or say — the view, the offer tree,
//! the app agent's readout, the console — and in what the agent hears.

use super::*;

/// A made-up network (plan: walks and tests never use a real one).
const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";

/// Connect over USB → the status is read once; set name and password through
/// the offer → the board holds them and the card reads them back; the
/// toggles change one setting each; Forget is Lasting and leaves `cloudRelay`.
/// Nothing Studio shows or logs holds the password.
#[test]
fn wifi_is_read_on_connect_set_through_its_offer_and_forgotten() {
    let device = empty_light_player("dev000000wifi00001");
    let (mut bench, tasks) = identified(&device, "usb-wifi-1");
    let target = bench.view().devices[0].id;

    let status = wifi_status(&mut bench, &tasks, target);
    assert_eq!(status.wifi, None, "a fresh board has no network");
    assert!(status.cloud_relay, "the relay is on by default");
    assert_eq!(status.station, crate::StationState::Unsupported);
    let set = wifi_verb(&mut bench, target, "set");
    assert!(bench.offered(&set).takes_a_secret());
    for verb in ["enabled", "forget"] {
        let path = wifi_verb(&mut bench, target, verb);
        bench.not_offered(path);
    }

    bench
        .press(
            &set,
            OfferArgs::new()
                .with(crate::WIFI_NETWORK_PARAM, SSID)
                .with(crate::WIFI_PASSWORD_PARAM, PASSWORD),
        )
        .expect("the set starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert_eq!(
        status.wifi,
        Some(crate::WifiInfo {
            ssid: SSID.to_string(),
            has_password: true,
            enabled: true,
        })
    );
    let wifi = bench.controller.device_roster_view().wifi[&target].clone();
    assert_eq!(wifi.row_value(), SSID);
    assert_eq!(
        wifi.status_line().as_deref(),
        Some(crate::app::network::SAVED_UNSUPPORTED)
    );
    assert_no_password_anywhere(&mut bench);

    // The board holds it: asked again (the popover's refresh), it answers
    // the same network — read off its own file, not Studio's memory.
    bench
        .controller
        .apply_network_command(crate::NetworkCommand::Refresh { device: target });
    assert_eq!(wifi_status(&mut bench, &tasks, target).wifi, status.wifi);

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
    let enabled = wifi_verb(&mut bench, target, "enabled");
    bench
        .press(
            enabled,
            OfferArgs::new().with(crate::WIFI_ENABLED_PARAM, "false"),
        )
        .expect("the switch starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert_eq!(status.wifi.as_ref().map(|wifi| wifi.enabled), Some(false));
    assert_eq!(
        status.wifi.as_ref().map(|wifi| wifi.has_password),
        Some(true),
        "a switch keeps the password"
    );

    // Forget: Lasting (the board forgets a password Studio never kept).
    let forget = wifi_verb(&mut bench, target, "forget");
    bench
        .press_lasting(forget, OfferArgs::new())
        .expect("the forget starts");
    let status = wifi_status(&mut bench, &tasks, target);
    assert_eq!(status.wifi, None);
    assert!(!status.cloud_relay, "cloudRelay outlives the network");
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
    let set = wifi_verb(&mut bench, target, "set");
    let refusal = bench
        .offered(&set)
        .press(
            &OfferArgs::new()
                .with(crate::WIFI_NETWORK_PARAM, SSID)
                .with(crate::WIFI_PASSWORD_PARAM, "short"),
        )
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("5 characters"), "{refusal}");
    assert!(!refusal.contains("short"), "{refusal}");
    assert_eq!(wifi_status(&mut bench, &tasks, target).wifi, None);
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
    let set = wifi_verb(&mut bench, target, "set").to_string();

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
    assert_eq!(
        wifi_status(&mut bench, &tasks, target).wifi,
        None,
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
    assert!(status.wifi.is_some_and(|wifi| wifi.has_password));
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
    assert!(!logs.contains(PASSWORD), "the console holds the password");
    let readout = bench.controller.app_agent_readout_for_test().render();
    assert!(
        !readout.contains(PASSWORD),
        "the readout holds the password"
    );
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
