//! Reset on a board reached over Wi‑Fi, pressed by its offer path.
//!
//! A socket has no reset lines, so the card used to draw Reset disabled
//! with "Reset needs USB" on every network link — although the board
//! restarts itself on the wire's `Reboot` over any of them. Now the
//! `devices/<board>/reset-board` offer is enabled over the LAN for the
//! author tier and sends that request; the board answers, resets, its
//! socket drops, the page's session redials, and the card comes back Ready
//! on the new link with no click. Over USB the same offer still pulses the
//! reset lines and never asks.

use super::ble_drop_tests::locked_store_file;
use super::lan_drop_tests::{DroppableLanBoard, wait_for_ready};
use super::*;

#[test]
fn reset_over_wifi_asks_the_board_and_the_card_comes_back() {
    // Open by default: anyone nearby holds the author tier.
    let device = lan_board("dev000000lanreset1", Vec::new());
    let (mut bench, tasks, lose) = over_wifi(&device);
    let card = bench.view().devices[0].clone();
    assert!(
        card.firmware_blocked.is_some(),
        "a LAN board still cannot be flashed from here"
    );

    // The link unlocks at the author tier (the board is open by default),
    // and from then on Reset is a verb that does something.
    let reset = bench.device_verb(card.id, "reset-board");
    bench.run_until(&tasks, "Reset to enable over Wi-Fi", |bench| {
        reset_enabled(bench, &reset)
    });
    assert_eq!(device.reboot_requests(), 0);
    bench.press_device(card.id, "reset-board", OfferArgs::new());
    bench.run_until(&tasks, "the board to be asked to restart", |_| {
        device.reboot_requests() == 1
    });

    // It resets: the socket drops, the page's session redials on its own
    // and says so on its presence edge, and the card is Ready again.
    lose.set(true);
    bench.run_until(&tasks, "the card to hear the drop", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label != "Ready")
    });
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    wait_for_ready(&mut bench, &tasks, "the card to come back on the new link");
    assert_eq!(bench.view().devices.len(), 1, "one board, one card");
    assert_eq!(device.reboot_requests(), 1, "asked once");
    bench.run_until(&tasks, "Reset to enable again on the new link", |bench| {
        reset_enabled(bench, &reset)
    });
}

/// A locked board (an author password, nobody let in without it) that
/// this browser has no password for: the board would refuse the request,
/// so Reset is drawn disabled and says why in plain words — never "needs
/// USB" — and nothing is sent.
#[test]
fn reset_over_wifi_without_the_author_tier_says_why() {
    let device = lan_board("dev000000lanreset2", vec![locked_store_file()]);
    let (mut bench, tasks, _lose) = over_wifi(&device);
    let card = bench.view().devices[0].clone();
    let reset = bench.device_verb(card.id, "reset-board");
    bench.run_until(&tasks, "the card to offer Reset", |bench| {
        bench.controller.view().offers.get(&reset).is_some()
    });
    assert_eq!(bench.offer_reason(&reset), crate::RESET_NEEDS_AUTHOR);
    assert_eq!(device.reboot_requests(), 0);
}

#[test]
fn reset_over_usb_still_pulses_the_lines_and_never_asks() {
    let device = empty_light_player("dev000000usbreset1");
    let (mut bench, tasks) = identified(&device, "usb-reset-1");
    let card = bench.view().devices[0].clone();
    let identifies_before = identify_count(&card);

    let reset = bench.device_verb(card.id, "reset-board");
    assert!(bench.offered(&reset).is_enabled());
    bench.press_device(card.id, "reset-board", OfferArgs::new());
    bench.run_until(&tasks, "the board to reboot and re-identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| identify_count(card) > identifies_before)
    });
    assert_eq!(
        device.reboot_requests(),
        0,
        "the line reset, not the wire's restart request"
    );
}

/// Whether the card offers `reset` enabled right now.
fn reset_enabled(bench: &DeviceBench, reset: &crate::OfferPath) -> bool {
    bench
        .controller
        .view()
        .offers
        .get(reset)
        .is_some_and(|offer| offer.is_enabled())
}

/// A LightPlayer whose link is an untrusted one (the board's access gate
/// asks a tier of every request), holding `root_files`.
fn lan_board(uid: &str, root_files: Vec<(String, Vec<u8>)>) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
            .with_base_mac("a0:f2:62:87:b4:8f")
            .with_heartbeat_interval(Duration::from_millis(20))
            .with_untrusted_link()
            .with_root_files(root_files),
    )))
}

/// A bench reaching `device` over the LAN only (no USB grant), identified;
/// the returned flag drops the open link, as the board's reset does.
fn over_wifi(device: &FakeEsp32Device) -> (DeviceBench, TaskPool, Rc<Cell<bool>>) {
    let (mut bench, tasks) = DeviceBench::build(device, "usb-unused", false, false);
    let lose = Rc::new(Cell::new(false));
    // The shipped build's shape: the LAN half joins the composite beside a
    // sim half (no sim is ever created here).
    bench
        .controller
        .set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
            ScriptedSimSource {
                device: sim_light_player(),
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            },
        ))));
    bench
        .controller
        .set_lan_transport(Rc::new(crate::LanDeviceTransport::new(Rc::new(
            DroppableLanBoard {
                device: device.clone(),
                lose: Rc::clone(&lose),
            },
        ))));
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    wait_for_ready(&mut bench, &tasks, "the board to identify over Wi-Fi");
    (bench, tasks, lose)
}
