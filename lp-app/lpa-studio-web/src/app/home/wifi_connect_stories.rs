//! Reaching a board on Wi‑Fi with no flag (the network-transport plan's
//! P01/P02): a remembered board's tile offering "Connect over Wi‑Fi", and
//! the add slot's "via Wi‑Fi" address field — each with what a connect
//! comes to, in core's words.
//!
//! Functional, not designed: a button and a line on the tile Studio already
//! draws for a remembered board, a field and a button in the add slot it
//! already has. The look is the device-UX rework's. Made-up addresses only.

use dioxus::prelude::*;
use lpa_studio_core::{
    BluetoothReach, DeviceEscape, DeviceRosterView, DeviceStatus, RosterView, UiHomeView,
    UiOfferTree, UiWifiConnect, WifiAddressReach, WifiConnectFailure, add_device_offers,
    new_sim_offer,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::usb_card;
use crate::app::home::device_offer_story_fixtures::StoryDevicesPage;
use crate::app::home::devices_page::AddDeviceCard;
use crate::core::OffersProvider;

#[story(
    description = "A board Studio met over USB, unplugged: it is on the remembered line (opened here), and because its Wi‑Fi status said it is on the network at 192.168.1.40 — learned over USB and kept in this browser, never in the registry — its tile offers \"Connect over Wi‑Fi\" (`devices/<board>/connect-wifi`, a core offer the app agent sees too) beside Reconnect and Forget. It wears the outline voice the tile's call to action wears. No flag: Studio installs the LAN link in every browser with a WebSocket."
)]
fn wifi_remembered_board_offers_connect() -> Element {
    remembered_tile(None)
}

#[story(
    description = "The same tile just after \"Connect over Wi‑Fi\" was pressed: the button waits (\"Connecting…\", disabled) and the line under the board says where — \"Connecting to 192.168.1.40…\". The socket is bounded (10 s), and the connect waits for the board's own first frame, so a board that turns the connection away is heard, not mistaken for connected. On success the board comes back as a card in the grid, the SAME device (merged by its MAC), its line reading \"Wi‑Fi · 192.168.1.40\"."
)]
fn wifi_remembered_board_connecting() -> Element {
    remembered_tile(Some(UiWifiConnect {
        host: "192.168.1.40".to_string(),
        through_relay: false,
        connecting: true,
        error: None,
    }))
}

#[story(
    description = "The same tile when nothing answered at the remembered address (the board is off, or on another network, or its address changed): the line says so in plain words — \"Couldn't reach the board at 192.168.1.40. Is it on this network?\" — and \"Connect over Wi‑Fi\" can be pressed again. Studio keeps no session redialling an address that did not answer."
)]
fn wifi_remembered_board_unreachable() -> Element {
    remembered_tile(Some(failed(
        "192.168.1.40",
        WifiConnectFailure::Unreachable {
            host: "192.168.1.40".to_string(),
        },
    )))
}

#[story(
    description = "The same tile when the board turned the connection away: its one Wi‑Fi slot is taken — Studio in another tab updating it, or lp-cli — so it closed the socket with \"try again later\" (1013). The line says so (\"Busy with another connection — try again\"), and \"Connect over Wi‑Fi\" can be pressed again once the other connection lets go."
)]
fn wifi_remembered_board_busy() -> Element {
    remembered_tile(Some(failed("192.168.1.40", WifiConnectFailure::Busy)))
}

#[story(
    description = "The add slot with its third way in (P02): under \"via USB\" and \"via Bluetooth\", one field for a board's address — an IP, or `lp-1a2b.local` where the browser resolves it — and Connect (`devices/connect-wifi-address`, one text parameter, normalised by core: a bare host, `ws://host`, `host:port`). No picker, no wizard. Connect waits for the field; nothing under it until something is typed."
)]
fn wifi_add_slot_address_field() -> Element {
    add_slot(None, None, false)
}

#[story(description = "The field with an address typed: Connect is pressable.")]
fn wifi_add_slot_address_typed() -> Element {
    add_slot(Some("192.168.1.40"), None, false)
}

#[story(
    description = "Not a board's address: `http://192.168.1.40/` is refused by core's offer before any socket opens, and Connect says why under it (a board is reached at ws://<host>/link). Credentials and an empty host are refused the same way."
)]
fn wifi_add_slot_not_an_address() -> Element {
    add_slot(Some("http://192.168.1.40/"), None, false)
}

#[story(
    description = "While a typed address is being reached: Connect waits (\"Connecting…\") and the line under it says where."
)]
fn wifi_add_slot_connecting() -> Element {
    add_slot(
        Some("192.168.1.40"),
        Some(UiWifiConnect {
            host: "192.168.1.40".to_string(),
            through_relay: false,
            connecting: true,
            error: None,
        }),
        true,
    )
}

#[story(
    description = "Each way a typed address can fail, in core's words, one slot per failure: the board already has a connection (its one network slot is taken: it closes the socket with \"try again later\", 1013); Chrome's Local Network check blocked a public page reaching a private address; nothing answered; and a `.local` name this browser could not resolve."
)]
fn wifi_add_slot_failures() -> Element {
    let failures = [
        ("192.168.1.40", WifiConnectFailure::Busy),
        ("192.168.1.40", WifiConnectFailure::Blocked),
        (
            "192.168.1.41",
            WifiConnectFailure::Unreachable {
                host: "192.168.1.41".to_string(),
            },
        ),
        (
            "lp-1a2b.local",
            WifiConnectFailure::NotFound {
                host: "lp-1a2b.local".to_string(),
            },
        ),
    ];
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:sm:grid-cols-2",
            for (host , failure) in failures {
                {add_slot(Some(host), Some(failed(host, failure)), false)}
            }
        }
    }
}

/// The devices page with one remembered board, its line open, the board's
/// remembered Wi‑Fi address known to this browser.
fn remembered_tile(connect: Option<UiWifiConnect>) -> Element {
    let mut card = usb_card();
    card.status = DeviceStatus::Offline;
    card.state_label = "Offline".to_string();
    card.freshness_label = Some("last heard 1 min ago".to_string());
    card.escapes = vec![DeviceEscape::Reconnect, DeviceEscape::Forget];
    let id = card.id;
    let devices = DeviceRosterView {
        transport_available: true,
        usb_available: true,
        wifi_connects: connect.into_iter().map(|connect| (id, connect)).collect(),
        roster: RosterView {
            pending: Vec::new(),
            devices: vec![card],
        },
        ..DeviceRosterView::default()
    };
    let home = UiHomeView {
        projects: Vec::new(),
        examples: Vec::new(),
        devices,
        library_available: true,
        opening: None,
        issue: None,
    };
    rsx! {
        section { class: "tw:max-w-[760px] tw:p-4",
            StoryDevicesPage {
                home,
                remembered_open: true,
                wifi_addresses: vec![(id, "192.168.1.40".to_string())],
                on_action: |_| {},
            }
        }
    }
}

/// The add slot alone, in Chrome on a computer, its Wi‑Fi field as typed and
/// its connect as core says it.
fn add_slot(typed: Option<&str>, connect: Option<UiWifiConnect>, connecting: bool) -> Element {
    let mut offers = UiOfferTree::new();
    let wifi = WifiAddressReach {
        available: true,
        connecting,
    };
    for offer in add_device_offers(true, BluetoothReach::Ready, wifi) {
        offers.publish(offer);
    }
    offers.publish(new_sim_offer());
    rsx! {
        div { class: "tw:max-w-[360px] tw:p-3",
            OffersProvider { offers,
                AddDeviceCard {
                    ble_reach: Some(BluetoothReach::Ready),
                    usb_available: true,
                    page_url: Some("https://lightplayer.app/devices".to_string()),
                    wifi_connect: connect,
                    wifi_typed: typed.map(str::to_string),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// A connect to `host` that failed for `failure`, as core says it.
fn failed(host: &str, failure: WifiConnectFailure) -> UiWifiConnect {
    UiWifiConnect {
        host: host.to_string(),
        through_relay: false,
        connecting: false,
        error: Some(failure.words()),
    }
}
