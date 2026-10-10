//! A board reached on the LAN (Wi-Fi M6 P07; no flag since the network
//! transport's P01): its device card, connected and just dropped.
//!
//! Functional, not designed — the card's look for network boards is the
//! device-UX rework's. What these pin is that a Wi-Fi board says how it is
//! reached ("Wi‑Fi · <address>", first on the device line), that its states are
//! the usual ones, and that its preview speaks for Wi‑Fi, never for
//! Bluetooth (the feed runs over a LAN link). Every one is also captured at
//! the phone width (the story harness's `sm` viewport). Made-up addresses
//! only.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceEscape, DeviceStatus, DeviceView, FIRMWARE_NEEDS_USB, UiDeviceAccess,
    lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::usb_card;
use crate::app::home::device_offer_story_fixtures::StoryDeviceCard;

#[story(
    description = "A board on the LAN (reached at `ws://192.168.1.40/link` — its remembered address, an address typed into Connect a board's Network row, or the `?lan=` dev shortcut), connected over a secure link that this browser's own key opened at edit: the connection bar leads with how it is reached — \"Wi‑Fi · connected\", the same word the editor header and every other surface use for the link — and its details give the address and URL, with no USB line: a USB or Bluetooth fact would describe a link this board is not on. Before a frame lands its picture is dark, and the status corner's details say what a USB board's do (\"No picture yet — the live feed is coming.\"): the card's feed runs over a LAN link. Before PR B's fix it said \"No live picture over Bluetooth\", because the model blocks firmware on every network link and the card read that as Bluetooth. Everything else is the card every running board wears."
)]
fn wifi_card_connected() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: wifi_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(unlocked()),
                lan: lan_link_for_endpoint(ENDPOINT),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "The same board the moment its socket dropped (it rebooted): \"Offline\" on the connection bar, Connect as the primary, the address and \"last heard 4 s ago\" in its details — the address the page keeps dialling, with no gesture, until the board answers and a new secure session says hello. (The drop's own words, \"wi-fi link lost: the board closed the link (code 1001: rebooting)\", are in the device journal; an offline card draws no terminal.)"
)]
fn wifi_card_dropped() -> Element {
    let mut card = wifi_card();
    card.status = DeviceStatus::Offline;
    card.state_label = "Offline".to_string();
    card.freshness_label = Some("last heard 4 s ago".to_string());
    card.escapes = vec![DeviceEscape::Reconnect, DeviceEscape::Forget];
    card.can_receive_project = false;
    card.can_remove_project = false;
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card,
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                lan: lan_link_for_endpoint(ENDPOINT),
                on_action: |_| {},
            }
        }
    }
}

const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";

/// The board both stories show.
const ENDPOINT: &str = "lan:ws://192.168.1.40/link";

/// The catalog choker, running, reached on the LAN.
fn wifi_card() -> DeviceView {
    DeviceView {
        // As the model has it: the USB flash is blocked on every network
        // link (no reset lines, no ROM downloader), Bluetooth and the LAN
        // alike. The LAN carries the update channel (OTA M8), so the
        // over-the-air update is not.
        firmware_blocked: Some(FIRMWARE_NEEDS_USB.to_string()),
        update_blocked: None,
        ..usb_card()
    }
}

/// Unlocked at edit by this browser's key (the link's own handshake).
fn unlocked() -> UiDeviceAccess {
    UiDeviceAccess {
        over_bluetooth: false,
        line: Some("Unlocked by Yona's MacBook".to_string()),
        unlock: None,
        panel: None,
        account_key_refused: None,
        grant: Some(lpa_studio_core::UiAccessGrant {
            tier: lpa_studio_core::AccessTier::Edit,
            key: Some("Yona's MacBook".to_string()),
        }),
        waiting: None,
    }
}
