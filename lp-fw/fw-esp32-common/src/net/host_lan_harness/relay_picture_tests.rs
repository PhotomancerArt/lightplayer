//! Relay protocol 2's pictures and project report, proven on the host
//! harness: the board's own relay driver, device leg, picture slot and
//! `serve_relay` (the C6's frame-hook answer, run by the harness's server
//! thread), against a stand-in hub ([`super::test_hub`]). The harness loads
//! no project, so every picture is the empty one (`0b 00 0000`) and the
//! project report is "none". The real hub's side is
//! `lp-cli/tests/relay_link.rs`.

extern crate std;

use alloc::string::String;
use std::time::{Duration, Instant};

use lpc_access::{SecretEntry, SecretKind, Tier};
use lpc_relay::{PictureRate, RelayState};

use super::test_hub::{HubEvent, TestHub};
use super::{HarnessAccess, HarnessRelay, LanHarness, LanHarnessOptions};
use crate::net::relay::RelayPictureMode;

const WAIT: Duration = Duration::from_secs(10);

/// None before the hub asks; one at once when it does; then the idle
/// cadence (a minute), so no second one soon. The project report (nothing
/// loaded) follows the registration.
#[test]
fn no_picture_before_a_rate_then_one_at_once() {
    let mut hub = TestHub::start();
    let harness = start(&hub);
    registered(&mut hub, &harness);
    assert_eq!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Project(_))),
        Some(HubEvent::Project(None)),
        "the board reports it plays nothing"
    );
    assert!(
        hub.wait_for(Duration::from_secs(1), is_picture).is_none(),
        "no picture before the hub asks"
    );
    assert_eq!(harness.relay_pictures(), (0, RelayPictureMode::Off));

    hub.send_rate(rate(60, 500, 0));
    let Some(HubEvent::Picture(picture)) = hub.wait_for(WAIT, is_picture) else {
        panic!("a picture at once");
    };
    assert!(picture.outputs.is_empty(), "no project, no lamps");
    assert!(picture.colors.is_empty());
    assert!(
        hub.wait_for(Duration::from_millis(1500), is_picture)
            .is_none(),
        "idle: the next one is a minute away"
    );
    assert_eq!(harness.relay_pictures(), (1, RelayPictureMode::Idle));
    harness.stop();
}

/// A watch makes pictures fast while it lasts (`watched_ms`, here 250),
/// then the board falls back to idle by itself.
#[test]
fn watched_pictures_come_fast_then_the_board_falls_back_to_idle() {
    let mut hub = TestHub::start();
    let harness = start(&hub);
    registered(&mut hub, &harness);

    hub.send_rate(rate(60, 250, 2));
    let started = Instant::now();
    let mut pictures = 0;
    let mut watched_seen = false;
    while started.elapsed() < Duration::from_millis(1900) {
        if hub
            .wait_for(Duration::from_millis(100), is_picture)
            .is_some()
        {
            pictures += 1;
        }
        watched_seen |= harness.relay_pictures().1 == RelayPictureMode::Watched;
    }
    assert!(watched_seen, "the heartbeat says watched during the watch");
    assert!(
        pictures >= 4,
        "one at once and one every 250 ms: {pictures} in 1.9 s"
    );
    // The watch ends at 2 s; drain what is in flight, then quiet.
    std::thread::sleep(Duration::from_millis(400));
    while hub
        .wait_for(Duration::from_millis(50), is_picture)
        .is_some()
    {}
    assert!(
        hub.wait_for(Duration::from_millis(1500), is_picture)
            .is_none(),
        "idle again after the watch"
    );
    let (sent, mode) = harness.relay_pictures();
    assert_eq!(mode, RelayPictureMode::Idle);
    assert!(sent >= 5, "{sent} pictures counted");
    harness.stop();
}

/// The leg going takes the picture buffers with it: a board off the relay
/// holds none.
#[test]
fn the_hub_dropping_the_leg_releases_the_picture_buffers() {
    let mut hub = TestHub::start();
    let harness = start(&hub);
    registered(&mut hub, &harness);
    hub.send_rate(rate(60, 500, 0));
    assert!(hub.wait_for(WAIT, is_picture).is_some(), "a picture");
    wait(&|| harness.relay_holds_a_picture_buffer(), "the spare kept");

    hub.drop_leg();
    assert_eq!(
        hub.wait_for(WAIT, |e| *e == HubEvent::LegClosed),
        Some(HubEvent::LegClosed)
    );
    wait(
        &|| !harness.relay_holds_a_picture_buffer(),
        "the buffers released",
    );
    harness.stop();
}

// --- helpers ---

fn start(hub: &TestHub) -> LanHarness {
    LanHarness::start(LanHarnessOptions {
        access: HarnessAccess::locked(alloc::vec![account()]),
        graphics: None,
        relay: Some(HarnessRelay {
            host: String::from("127.0.0.1"),
            port: hub.port,
            board_mac: [0x02, 0, 0, 0, 0, 2],
            label: String::from("harness"),
        }),
    })
    .expect("the harness starts")
}

fn registered(hub: &mut TestHub, harness: &LanHarness) {
    assert!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Registered { .. }))
            .is_some(),
        "the board registers"
    );
    wait(
        &|| harness.relay_status().0 == RelayState::Connected,
        "connected",
    );
}

fn wait(done: &dyn Fn() -> bool, what: &str) {
    let until = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < until, "never: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn is_picture(event: &HubEvent) -> bool {
    matches!(event, HubEvent::Picture(_))
}

fn rate(idle_s: u16, watched_ms: u16, watched_for_s: u16) -> PictureRate {
    PictureRate {
        idle_s,
        watched_ms,
        watched_for_s,
    }
}

/// An account key, as Studio installs it.
fn account() -> SecretEntry {
    SecretEntry {
        label: String::from("test key"),
        kind: SecretKind::Account,
        tier: Tier::Edit,
        salt: [3; 16],
        iterations: 1,
        k: [103; 32],
        added_at: None,
    }
}
