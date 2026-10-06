//! A board reached on the LAN (`?lan=`, Wi-Fi M6 P07): its device card,
//! connected and just dropped.
//!
//! Functional, not designed — the card's look for network boards is
//! roadmap M8. What these pin is that a Wi-Fi board says how it is reached
//! ("Wi-Fi · <address>", first on the device line) and that its states are
//! the usual ones. Every one is also captured at the phone width (the story
//! harness's `sm` viewport). Made-up addresses only.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceEscape, DeviceStatus, DeviceView, UiDeviceAccess, lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::usb_card;
use crate::app::home::device_offer_story_fixtures::StoryDeviceCard;

#[story(
    description = "A board on the LAN (`?lan=ws://192.168.1.40/link`), connected over a secure link that this browser's own key opened at edit: the device line leads with how it is reached — \"Wi-Fi · 192.168.1.40\" — then \"Unlocked by Yona's MacBook\" and the freshness. No Connections group: its USB and Bluetooth rows would describe links this board is not on. Everything else is the card every running board wears; the card's network look is M8's."
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
    description = "The same board the moment its socket dropped (it rebooted): \"Offline\", Reconnect and Forget, the project and firmware verbs gone, and the device line still \"Wi-Fi · 192.168.1.40 · last heard 4 s ago\" — the address the page keeps dialling, with no gesture, until the board answers again and a new secure session says hello. (The drop's own words, \"wi-fi link lost: the board closed the link (code 1001: rebooting)\", are in the device journal; an offline card draws no terminal.)"
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

/// The board both stories show, as `?lan=` names it.
const ENDPOINT: &str = "lan:ws://192.168.1.40/link";

/// The catalog choker, running, reached on the LAN.
fn wifi_card() -> DeviceView {
    DeviceView {
        // A LAN link carries no update channel and the model blocks nothing
        // by name over it yet; the transport refuses a flash with the reason.
        firmware_blocked: None,
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
    }
}
