//! Wi‑Fi stories (Wi‑Fi roadmap M5, plan P5): the card's Wi‑Fi row, and the
//! Wi‑Fi panel in every state a board can answer — not set, saved on a
//! firmware that cannot join yet (every M5 image), M6's joining / joined /
//! failed, the cloud relay off, reading, a refusal in the board's words, a link below
//! author, a change in flight, and Forget armed.
//!
//! Functional, not designed: the UX pass is later. Every one is also
//! captured at the phone width (the story harness's `sm` viewport).
//! Made-up values only (`lp-walk-net` / `correct-horse-42`).

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceId, NetworkStatus, OfferArgs, OfferPath, StationState, UiDeviceWifi, UiOffer, WifiInfo,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::{usb_access, usb_card};
use crate::app::home::device_offer_story_fixtures::StoryDeviceCard;
use crate::app::home::wifi_panel::WifiPanel;

// --- 1 · The row ------------------------------------------------------------

#[story(
    description = "The device card's Connections group over USB with a network saved: under USB, Bluetooth and Access, a \"Wi‑Fi\" row names the network (\"lp-walk-net ›\") and opens the Wi‑Fi panel."
)]
fn wifi_row_on_the_card() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), false)),
                wifi: Some(wifi(saved(StationState::Unsupported, true))),
                on_action: |_| {},
            }
        }
    }
}

// --- 2 · The panel ----------------------------------------------------------

#[story(
    description = "No network saved: \"Not set.\", the relay line, and the form — a network name (required) and a password field (dots, Show/Hide; blank means an open network). No Forget and no on/off switch until something is saved; the Cloud relay switch (on by default, \"Lets lightplayer.app reach this board through the cloud.\") is always there."
)]
fn wifi_not_set() -> Element {
    panel(wifi(not_set()), None)
}

#[story(
    description = "The M5 reality: a network saved on a firmware that does not join yet — \"Saved. This firmware doesn't join Wi‑Fi yet.\" The name field shows the saved name as its placeholder; the password field says \"unchanged\" (the board never gives the password back). Join this network, Cloud relay, and Forget below."
)]
fn wifi_saved_unsupported() -> Element {
    panel(wifi(saved(StationState::Unsupported, true)), None)
}

#[story(
    description = "Typing a new network: the name, and the password as dots (the field is a password input with Show/Hide; it is cleared once pressed and never echoed). Save is live."
)]
fn wifi_typing_a_network() -> Element {
    panel(
        wifi(not_set()),
        Some(
            OfferArgs::new()
                .with(lpa_studio_core::WIFI_NETWORK_PARAM, "lp-walk-net")
                .with(lpa_studio_core::WIFI_PASSWORD_PARAM, "correct-horse-42"),
        ),
    )
}

#[story(
    description = "M6's states, ready before any firmware produces them: joining (\"Joining lp-walk-net…\"), joined (\"Joined lp-walk-net · 192.168.1.40 · -58 dBm\"), and failed (\"Couldn't join lp-walk-net: wrong password\")."
)]
fn wifi_station_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:sm:grid-cols-3",
            {panel(wifi(saved(StationState::Joining, true)), None)}
            {panel(wifi(saved(StationState::Joined { ip: "192.168.1.40".to_string(), rssi: -58 }, true)), None)}
            {panel(wifi(saved(StationState::Failed { reason: "wrong password".to_string() }, true)), None)}
        }
    }
}

#[story(
    description = "The cloud relay switched off: \"Relay off — local network only\" (with \"applies once this firmware uses the relay\" while the firmware does not join), and the Cloud relay switch off."
)]
fn wifi_cloud_relay_off() -> Element {
    panel(wifi(saved(StationState::Unsupported, false)), None)
}

#[story(
    description = "Reading: the panel opened before the board answered — \"Reading…\" and no verbs yet."
)]
fn wifi_reading() -> Element {
    let mut reading = wifi(not_set());
    reading.status = None;
    reading.reading = true;
    panel(reading, None)
}

#[story(
    description = "A change the board refused: its sentence under the form, in red, beside the status it last answered — it names the rule, never the password."
)]
fn wifi_refused() -> Element {
    let mut refused = wifi(not_set());
    refused.error = Some(
        "cannot save the network: the password is 7 characters; Wi-Fi needs at least 8 (or none for an open network)"
            .to_string(),
    );
    panel(refused, None)
}

#[story(
    description = "A Bluetooth link unlocked for play only: the board is not asked, and the panel says what it needs — \"Needs Author access — unlock with an author password.\" No verbs."
)]
fn wifi_needs_author() -> Element {
    let mut play = wifi(saved(StationState::Unsupported, true));
    play.can_edit = false;
    play.status = None;
    panel(play, None)
}

#[story(
    description = "A change on its way: \"Writing to the device…\", and every verb drawn disabled (never hidden) until the board answers."
)]
fn wifi_writing() -> Element {
    let mut writing = wifi(saved(StationState::Unsupported, true));
    writing.writing = true;
    panel(writing, None)
}

#[story(
    description = "Forget armed (the user's first click): it is Lasting — the board forgets lp-walk-net and its password, and only the user can bring the password back."
)]
fn wifi_forget_armed() -> Element {
    let wifi = wifi(saved(StationState::Unsupported, true));
    let offers = offers(&wifi);
    rsx! {
        div { class: PANEL_FRAME,
            WifiPanel { wifi, offers, on_action: |_| {}, forget_armed_preview: true }
        }
    }
}

// --- fixtures ---------------------------------------------------------------

/// The panel in the popover's frame, with the verbs core would publish.
fn panel(wifi: UiDeviceWifi, args: Option<OfferArgs>) -> Element {
    let offers = offers(&wifi);
    rsx! {
        div { class: PANEL_FRAME,
            WifiPanel { wifi, offers, on_action: |_| {}, args_preview: args }
        }
    }
}

/// Core's Wi‑Fi offers for `wifi`, at the story board's prefix.
fn offers(wifi: &UiDeviceWifi) -> Vec<UiOffer> {
    let prefix = OfferPath::parse("devices/mac-a0f26287b48c").expect("a board prefix");
    lpa_studio_core::app::network::wifi_offers(&prefix, wifi)
}

fn wifi(status: NetworkStatus) -> UiDeviceWifi {
    UiDeviceWifi {
        device: DeviceId(7),
        can_edit: true,
        status: Some(status),
        reading: false,
        writing: false,
        error: None,
    }
}

fn not_set() -> NetworkStatus {
    NetworkStatus {
        wifi: None,
        cloud_relay: true,
        station: StationState::Unsupported,
    }
}

fn saved(station: StationState, cloud_relay: bool) -> NetworkStatus {
    NetworkStatus {
        wifi: Some(WifiInfo {
            ssid: "lp-walk-net".to_string(),
            has_password: true,
            enabled: true,
        }),
        cloud_relay,
        station,
    }
}

/// The detail card's own frame (the access panel stories' width).
const PANEL_FRAME: &str = "tw:m-3 tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:text-sm tw:text-muted-foreground ux-glass-panel";

/// One card, at most the roster column's width.
const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";
