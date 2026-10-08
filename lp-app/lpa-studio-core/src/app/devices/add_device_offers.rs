//! `devices/connect-usb`, `devices/connect-ble` and
//! `devices/connect-wifi-address`: the add slot's three ways a board comes
//! in, each named for what it does.
//!
//! All three are ALWAYS offered (G3, 2026-09-24): a transport this browser
//! cannot drive is disabled with its reason rather than missing, so the add
//! slot draws the button with the sentence under it — and the app agent
//! reads the same sentence. The way forward the slot adds under a disabled
//! button (a link to Bluefy, the Brave flag, this page's address to copy) is
//! the web's; the reason is core's.
//!
//! USB and Bluetooth need the user's click: each opens the browser's own
//! chooser. Wi‑Fi does not — there is no chooser for a socket — so it takes
//! the board's address as one text parameter instead (`address`: an IP, or
//! `lp-1a2b.local`), normalised by [`normalize_lan_address`] and refused by
//! name when it is not a board's address.

use lpa_devices::Action;

use super::bluetooth_reach::BluetoothReach;
use super::devices_op::DevicesOp;
use super::lan_addresses::normalize_lan_address;
use super::wifi_connect_op::WifiConnectOp;
use crate::{OfferArgError, OfferArgs, OfferBinder, OfferParam, OfferPath, UiOffer};

/// Why "via USB" cannot be pressed in a browser without Web Serial (iPhone,
/// iPad, Bluefy, Firefox, Safari).
pub const USB_NEEDS_WEB_SERIAL: &str = "USB needs Chrome or Edge on a computer.";

/// Why "Connect a board on Wi‑Fi" cannot be pressed in a page with no
/// WebSocket (every browser Studio runs in has one; a host build does not).
pub const WIFI_NEEDS_WEBSOCKET: &str =
    "Wi\u{2011}Fi boards need a browser that can open a WebSocket.";

/// Why "Connect a board on Wi‑Fi" waits: one address is being reached.
pub const WIFI_CONNECTING: &str = "Connecting\u{2026}";

/// The address parameter of `devices/connect-wifi-address`.
pub const WIFI_ADDRESS_PARAM: &str = "address";

/// What the add slot's Wi‑Fi entry can do right now.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WifiAddressReach {
    /// This page reaches the LAN (a WebSocket, the LAN transport installed).
    pub available: bool,
    /// A typed address is being reached now.
    pub connecting: bool,
}

/// The add slot's transports, USB first: `usb_available` is whether this
/// page has Web Serial (or the `?emu=` shim standing in for it), `bluetooth`
/// what the browser answered about Bluetooth, `wifi` whether a typed
/// address can be reached.
pub fn add_device_offers(
    usb_available: bool,
    bluetooth: BluetoothReach,
    wifi: WifiAddressReach,
) -> Vec<UiOffer> {
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
        connect_wifi_address_offer(wifi),
    ]
}

/// `devices/connect-wifi-address`: reach whatever board answers at a typed
/// address. One required text parameter; the binder dials only what
/// [`normalize_lan_address`] accepts.
fn connect_wifi_address_offer(wifi: WifiAddressReach) -> UiOffer {
    let waits = wifi_waits(wifi);
    let unbound = WifiConnectOp::action_for(WifiConnectOp::Address { url: String::new() });
    let mut offer = UiOffer::with_params(
        OfferPath::devices().child("connect-wifi-address"),
        "wifi",
        vec![OfferParam::text(
            WIFI_ADDRESS_PARAM,
            "board's address",
            "192.168.1.40 or lp-1a2b.local",
        )],
        OfferBinder::new(move |args| bind_wifi_address(args, waits)),
        unbound,
    );
    // Until an address is typed the verb reads "choose a board's address";
    // a page that cannot reach the LAN, or one reaching an address now,
    // says that instead.
    if let Some(reason) = waits {
        offer.action = offer.action.disabled(reason);
    }
    offer
}

/// Why the add slot's Wi‑Fi entry cannot be pressed now, if it cannot.
fn wifi_waits(wifi: WifiAddressReach) -> Option<&'static str> {
    if !wifi.available {
        Some(WIFI_NEEDS_WEBSOCKET)
    } else if wifi.connecting {
        Some(WIFI_CONNECTING)
    } else {
        None
    }
}

/// The connect for a typed address, or why it is not one.
fn bind_wifi_address(
    args: &OfferArgs,
    waits: Option<&'static str>,
) -> Result<crate::UiAction, OfferArgError> {
    let typed = args.text(WIFI_ADDRESS_PARAM).unwrap_or_default().trim();
    if typed.is_empty() {
        return Err(OfferArgError::Missing {
            name: WIFI_ADDRESS_PARAM.to_string(),
            label: "board's address".to_string(),
        });
    }
    let url = normalize_lan_address(typed).map_err(|reason| OfferArgError::Invalid {
        name: WIFI_ADDRESS_PARAM.to_string(),
        reason,
    })?;
    let action = WifiConnectOp::action_for(WifiConnectOp::Address { url });
    Ok(match waits {
        Some(reason) => action.disabled(reason),
        None => action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transport_is_offered_and_disabled_with_its_reason() {
        let paths = |offers: &[UiOffer]| {
            offers
                .iter()
                .map(|offer| offer.path.to_string())
                .collect::<Vec<_>>()
        };
        let ready = add_device_offers(true, BluetoothReach::Ready, reachable());
        assert_eq!(
            paths(&ready),
            [
                "devices/connect-usb",
                "devices/connect-ble",
                "devices/connect-wifi-address"
            ]
        );
        assert!(ready[..2].iter().all(UiOffer::is_enabled));
        // The address entry reads as waiting for its one field, the way
        // every offer with a required value does until it is typed.
        assert_eq!(
            ready[2].action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: "choose a board's address".to_string()
            }
        );
        assert!(ready.iter().all(|offer| offer.consequence().is_routine()));

        let phone = add_device_offers(false, BluetoothReach::Ios, WifiAddressReach::default());
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
        assert_eq!(
            phone[2].action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: WIFI_NEEDS_WEBSOCKET.to_string()
            }
        );
    }

    #[test]
    fn an_address_is_normalised_to_the_socket_studio_dials() {
        let offer = wifi_offer(reachable());
        assert_eq!(offer.action.meta().label, "Connect a board on Wi\u{2011}Fi");
        for (typed, url) in [
            ("192.168.1.40", "ws://192.168.1.40/link"),
            (" 192.168.1.40 ", "ws://192.168.1.40/link"),
            ("ws://192.168.1.40", "ws://192.168.1.40/link"),
            ("lp-1A2B.local", "ws://lp-1a2b.local/link"),
            ("127.0.0.1:28111", "ws://127.0.0.1:28111/link"),
            ("ws://10.0.0.5/link", "ws://10.0.0.5/link"),
        ] {
            let action = offer
                .press(&OfferArgs::new().with(WIFI_ADDRESS_PARAM, typed))
                .unwrap_or_else(|error| panic!("{typed}: {error}"));
            assert_eq!(
                action.op_as::<WifiConnectOp>(),
                Some(&WifiConnectOp::Address {
                    url: url.to_string()
                }),
                "{typed}"
            );
        }
    }

    #[test]
    fn what_is_not_a_boards_address_is_refused_with_a_short_reason() {
        let offer = wifi_offer(reachable());
        for (typed, says) in [
            ("http://10.0.0.5/", "not http://"),
            ("ws://user:pw@10.0.0.5/link", "no credentials"),
            ("ws:///link", "no host"),
        ] {
            let refused = offer
                .press(&OfferArgs::new().with(WIFI_ADDRESS_PARAM, typed))
                .expect_err(typed)
                .to_string();
            assert!(refused.contains(says), "{typed}: {refused}");
        }
        let empty = offer
            .press(&OfferArgs::new().with(WIFI_ADDRESS_PARAM, "  "))
            .expect_err("an empty field");
        assert!(matches!(empty, OfferArgError::Missing { .. }), "{empty}");
    }

    #[test]
    fn the_field_waits_while_an_address_is_being_reached() {
        let offer = wifi_offer(WifiAddressReach {
            available: true,
            connecting: true,
        });
        assert_eq!(
            offer.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: WIFI_CONNECTING.to_string()
            }
        );
        let refused = offer
            .press(&OfferArgs::new().with(WIFI_ADDRESS_PARAM, "10.0.0.5"))
            .expect_err("one address at a time");
        assert_eq!(
            refused,
            OfferArgError::Unavailable {
                reason: WIFI_CONNECTING.to_string()
            }
        );
    }

    fn reachable() -> WifiAddressReach {
        WifiAddressReach {
            available: true,
            connecting: false,
        }
    }

    fn wifi_offer(wifi: WifiAddressReach) -> UiOffer {
        add_device_offers(true, BluetoothReach::Ready, wifi).remove(2)
    }
}
