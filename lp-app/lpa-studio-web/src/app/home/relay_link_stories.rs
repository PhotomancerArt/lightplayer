//! A board reached through lightplayer.app's relay, and a board that cannot
//! be (the network transport's PR C): the device card's link words, and
//! what a USB card says when the account's key did not go on.
//!
//! Functional, not designed — the card's look for network boards is the
//! device-UX rework's. What these pin is that a relay board says how it is
//! reached ("Wi‑Fi via lightplayer.app") and never wears Bluetooth's or
//! USB's words, and that a board kept off lightplayer.app by a full access
//! list says so on its card. Made-up names only.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceView, FIRMWARE_NEEDS_USB, UiDeviceAccess, account_key_refused_sentence,
    lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::{usb_access, usb_card};
use crate::app::home::device_offer_story_fixtures::StoryDeviceCard;

#[story(
    description = "A board reached through lightplayer.app's relay (`relay:<mac>`, opened by \"Connect through lightplayer.app\" or the `?relay=<mac>` shortcut), unlocked at edit by the account's key: the device line leads with \"Wi‑Fi via lightplayer.app\" — the same words the editor header uses — then the unlock and the freshness. No Connections group: its USB and Bluetooth rows would describe links this board is not on (behind `?relay=1` the card said \"USB connected\"). Its preview, before a frame lands, is a running board's (\"No picture yet — the live feed is coming.\"), never \"No live picture over Bluetooth\", which it said before PR C. Firmware verbs need USB, as on every network link."
)]
fn relay_card_connected() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: relay_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(UiDeviceAccess {
                    over_bluetooth: false,
                    line: Some("Unlocked by Yona's account".to_string()),
                    unlock: None,
                    panel: None,
                    account_key_refused: None,
                    grant: None,
                    waiting: None,
                }),
                lan: lan_link_for_endpoint("relay:a0f26287b48c"),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A board plugged in over USB while signed in, whose access list is full of passwords and account keys — nothing Studio may drop to make room (it drops the oldest other browser's key when there is one). The account's key could not go on, so the board can never be reached through lightplayer.app; the card says so under its Connections group, with the reason, instead of only inside the Access panel: \"Your account's key couldn't be added, so this board can't be reached through lightplayer.app. This device is full, and nothing on it can make room on its own — remove something from its list.\""
)]
fn usb_card_account_key_refused() -> Element {
    let mut access = usb_access(Some(true), false);
    access.account_key_refused = Some(account_key_refused_sentence(
        "This device is full, and nothing on it can make room on its own — remove something \
         from its list.",
    ));
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(access),
                on_action: |_| {},
            }
        }
    }
}

const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";

/// The catalog choker, running, reached through the relay.
fn relay_card() -> DeviceView {
    DeviceView {
        // As the model has it: the USB flash is blocked on every network
        // link; the update channel runs through the relay (OTA).
        firmware_blocked: Some(FIRMWARE_NEEDS_USB.to_string()),
        update_blocked: None,
        ..usb_card()
    }
}
