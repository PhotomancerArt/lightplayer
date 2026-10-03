//! `devices/connect-usb` and `devices/connect-ble`: the add slot's two ways
//! a board comes in, each named for what it does.
//!
//! Both are ALWAYS offered (G3, 2026-09-24): a transport this browser cannot
//! drive is disabled with its reason rather than missing, so the add slot
//! draws the button with the sentence under it — and the app agent reads
//! the same sentence. The way forward the slot adds under a disabled button
//! (a link to Bluefy, the Brave flag, this page's address to copy) is the
//! web's; the reason is core's.
//!
//! Both need the user's click: each opens the browser's own chooser.

use lpa_devices::Action;

use super::bluetooth_reach::BluetoothReach;
use super::devices_op::DevicesOp;
use crate::{OfferPath, UiOffer};

/// Why "via USB" cannot be pressed in a browser without Web Serial (iPhone,
/// iPad, Bluefy, Firefox, Safari).
pub const USB_NEEDS_WEB_SERIAL: &str = "USB needs Chrome or Edge on a computer.";

/// The add slot's two transports, USB first: `usb_available` is whether
/// this page has Web Serial (or the `?emu=` shim standing in for it),
/// `bluetooth` what the browser answered about Bluetooth.
pub fn add_device_offers(usb_available: bool, bluetooth: BluetoothReach) -> Vec<UiOffer> {
    let usb = DevicesOp::action_for(Action::AddFromUsb).with_label("Connect a board via USB");
    let ble = DevicesOp::action_for(Action::AddFromBle).with_label("Connect a board via Bluetooth");
    vec![
        UiOffer::new(
            OfferPath::devices().child("connect-usb"),
            "usb",
            match usb_available {
                true => usb,
                false => usb.disabled(USB_NEEDS_WEB_SERIAL),
            },
        ),
        UiOffer::new(
            OfferPath::devices().child("connect-ble"),
            "bluetooth",
            match bluetooth.disabled_reason() {
                Some(reason) => ble.disabled(reason),
                None => ble,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_transports_are_offered_and_disabled_with_their_reasons() {
        let paths = |offers: &[UiOffer]| {
            offers
                .iter()
                .map(|offer| offer.path.to_string())
                .collect::<Vec<_>>()
        };
        let ready = add_device_offers(true, BluetoothReach::Ready);
        assert_eq!(
            paths(&ready),
            ["devices/connect-usb", "devices/connect-ble"]
        );
        assert!(ready.iter().all(UiOffer::is_enabled));
        assert!(ready.iter().all(|offer| offer.consequence().is_routine()));

        let phone = add_device_offers(false, BluetoothReach::Ios);
        assert_eq!(
            phone[0].action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: USB_NEEDS_WEB_SERIAL.to_string()
            }
        );
        assert_eq!(
            phone[1].action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: BluetoothReach::Ios.reason().unwrap().to_string()
            }
        );
    }
}
