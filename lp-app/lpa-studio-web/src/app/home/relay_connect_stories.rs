//! Reaching a known board through lightplayer.app with no flag (the network
//! transport's PR C): a remembered board's tile offering "Connect through
//! lightplayer.app" while someone is signed in, and what a connect comes to,
//! in core's words.
//!
//! The least way in: one button on the tile Studio already draws for a
//! board it has met. No list of the account's boards, no sweep, no move onto
//! the LAN — those are the device-UX rework's. Made-up addresses only.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceEscape, DeviceRosterView, DeviceStatus, RELAY_NO_HELD_KEY_WORDS, RELAY_OFFLINE_WORDS,
    RelayConnectFailure, RosterView, UiHomeTab, UiHomeView, UiWifiConnect,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::usb_card;
use crate::app::home::device_offer_story_fixtures::StoryHomePage;

#[story(
    description = "A board Studio met over USB, unplugged, while someone is signed in: its tile offers \"Connect through lightplayer.app\" (`devices/<board>/connect-relay`, a core offer the app agent sees too) beside \"Connect over Wi‑Fi\" (this browser remembers its address) and Reconnect and Forget. The relay is on for everyone — no `?relay=` flag. Signed out, the button is not there: only the account's key, put on the board when it was plugged in signed in, opens it through lightplayer.app."
)]
fn relay_remembered_board_offers_connect() -> Element {
    remembered_tile(None, true)
}

#[story(
    description = "A board Studio met over Bluetooth (so no Wi‑Fi address is remembered): \"Connect through lightplayer.app\" is its only way back with no cable. Nothing asks lightplayer.app first whether the board is online — the press finds out."
)]
fn relay_remembered_board_without_an_address() -> Element {
    remembered_tile(None, false)
}

#[story(
    description = "Just after \"Connect through lightplayer.app\" was pressed: the button waits (disabled, \"Connecting through lightplayer.app…\") and the line under the board says the same. The connect waits for the board to accept one of this browser's keys, not just for lightplayer.app to answer; on success the board comes back as a card in the grid, the SAME device (merged by its MAC), its line reading \"Wi‑Fi via lightplayer.app\"."
)]
fn relay_remembered_board_connecting() -> Element {
    remembered_tile(
        Some(UiWifiConnect {
            host: "lightplayer.app".to_string(),
            through_relay: true,
            connecting: true,
            error: None,
            busy: false,
        }),
        true,
    )
}

#[story(
    description = "lightplayer.app answered that the board is not connected to it (close 4404): it is off, off its network, or its Cloud relay is off. The tile says so in plain words — \"The board isn't online.\" — and the button can be pressed again. Studio does not keep redialling a board the relay turned away."
)]
fn relay_remembered_board_offline() -> Element {
    remembered_tile(Some(failed(RELAY_OFFLINE_WORDS)), true)
}

#[story(
    description = "No key this browser holds opens the board through lightplayer.app (it was never plugged in while this account was signed in, or its account key was removed): \"Sign in to Studio and plug this board in once to reach it through lightplayer.app.\" No password is ever typed through the relay."
)]
fn relay_remembered_board_no_key() -> Element {
    remembered_tile(Some(failed(RELAY_NO_HELD_KEY_WORDS)), true)
}

#[story(
    description = "The board's one network connection is taken (Studio in another browser, or lp-cli, on the LAN or through the relay): \"Busy with another connection — try again\"."
)]
fn relay_remembered_board_busy() -> Element {
    remembered_tile(Some(failed(&RelayConnectFailure::Busy.words())), true)
}

/// The home page's Boards tab with one board under Offline boards, offered
/// "Connect through lightplayer.app" — and "Connect over Wi‑Fi" too when
/// this browser remembers its address (`with_address`).
fn remembered_tile(connect: Option<UiWifiConnect>, with_address: bool) -> Element {
    let mut card = usb_card();
    card.status = DeviceStatus::Offline;
    card.state_label = "Offline".to_string();
    card.freshness_label = Some("last heard 2 h ago".to_string());
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
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    };
    let wifi_addresses = match with_address {
        true => vec![(id, "192.168.1.40".to_string())],
        false => Vec::new(),
    };
    rsx! {
        section { class: "tw:max-w-[760px] tw:p-4",
            StoryHomePage {
                home,
                initial_tab: Some(UiHomeTab::Boards),
                wifi_addresses,
                relay_boards: vec![id],
                on_action: |_| {},
            }
        }
    }
}

/// A connect through the relay that failed with `words` (core's).
fn failed(words: &str) -> UiWifiConnect {
    UiWifiConnect {
        host: "lightplayer.app".to_string(),
        through_relay: true,
        connecting: false,
        error: Some(words.to_string()),
        busy: false,
    }
}
