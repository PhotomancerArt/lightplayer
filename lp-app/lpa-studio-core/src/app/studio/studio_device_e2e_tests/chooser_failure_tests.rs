//! Connect a board's USB or Bluetooth press, picked in the browser's
//! chooser and then failed: the section says so under its squares, and the
//! next press clears it. Before 2026-10-17 it was one console warning
//! (`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).
//!
//! The bench's transport has no Bluetooth of its own, so its chooser fails
//! the way a browser's would (the trait's default answer).

use super::*;

#[test]
fn a_bluetooth_connect_that_fails_after_the_pick_says_so_under_the_squares() {
    let device = light_player("dev_ble_fail");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-chooser-fail");
    bench
        .controller
        .set_bluetooth_reach(crate::BluetoothReach::Ready);
    assert_eq!(bench.controller.device_roster_view().chooser_failure, None);

    bench
        .press("devices/connect-ble", OfferArgs::new())
        .expect("the chooser press never fails loudly");
    bench.run_until(&tasks, "the failed chooser to say so", |bench| {
        bench
            .controller
            .device_roster_view()
            .chooser_failure
            .is_some()
    });

    let failure = bench
        .controller
        .device_roster_view()
        .chooser_failure
        .expect("said");
    assert_eq!(failure.transport, crate::ChooserTransport::Bluetooth);
    assert_eq!(
        failure.words,
        "Couldn't connect over Bluetooth (this browser cannot reach Bluetooth devices). \
         Press Bluetooth to try again."
    );
    assert!(bench.view().devices.is_empty());
}

#[test]
fn the_next_press_clears_the_failure() {
    let device = light_player("dev_ble_fail_then_usb");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-chooser-then");
    bench
        .controller
        .set_bluetooth_reach(crate::BluetoothReach::Ready);
    bench
        .press("devices/connect-ble", OfferArgs::new())
        .expect("the chooser press never fails loudly");
    bench.run_until(&tasks, "the failed chooser to say so", |bench| {
        bench
            .controller
            .device_roster_view()
            .chooser_failure
            .is_some()
    });

    bench
        .press("devices/connect-usb", OfferArgs::new())
        .expect("the chooser press never fails loudly");
    assert_eq!(bench.controller.device_roster_view().chooser_failure, None);
    bench.run_until(&tasks, "the chosen port to identify", |bench| {
        !bench.view().devices.is_empty()
    });
    assert_eq!(bench.controller.device_roster_view().chooser_failure, None);
}

#[test]
fn a_dismissed_chooser_says_nothing() {
    let device = light_player("dev_ble_dismiss");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-chooser-dismiss");
    bench.chooser_grants.set(false);
    bench
        .press("devices/connect-usb", OfferArgs::new())
        .expect("the chooser press never fails loudly");
    for _ in 0..50 {
        bench.step(&tasks);
    }
    assert_eq!(bench.controller.device_roster_view().chooser_failure, None);
}
