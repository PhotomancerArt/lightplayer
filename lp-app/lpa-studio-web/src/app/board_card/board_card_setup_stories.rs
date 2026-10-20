//! The board card's eight rows as a new board's setup list (plan
//! `2026-10-19-1350-board-setup-rows`, the spike `spikes/install-network-setup`):
//! the lit to-do rows across a board's first minutes, every state of the
//! four link rows, each radio row's own details led by its switch line,
//! the card at a phone's two-up width, and the name renamed where
//! it is.
//!
//! Every card is core's own (`StoryBoardCard` calls `board_card` over the
//! tree core would publish), so a story card says and offers exactly what
//! core decides.

use dioxus::prelude::*;
use lpa_studio_core::{
    BarLayer, BoardPlays, DeviceId, DeviceLoadedProject, DeviceView, NetworkStatus,
    SavedNetworkInfo, StationState, UiDeviceWifi, UiLinkKind, UiRuntimeBand, lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;
use lpc_wire::RelayState;
use lpc_wire::server::StationFailure;

use super::board_card_stories::{
    DAY, PORCH_UID, emulated, last_picture, live_feed, needs_firmware, offline, over_network,
    porch, running,
};
use super::{CardPart, CardPreviews};
use crate::app::home::ble_access_stories::usb_access;
use crate::app::home::device_offer_story_fixtures::{STORY_BOARD_NOW, StoryBoardCard};

// --- A new board's first minutes --------------------------------------------

#[story(
    description = "A new board's setup is its card (AC4, AC6): the rows with something left to do are LIT — the whole row on a lighter flat ground, its icon, kind and name at full strength, its button on the same ground with no fill of its own; no gradient, no dot. INSTALLED, SIGNED OUT: Project none · Add, Wi‑Fi not connected · Set up, lightplayer.app not connected · Sign in are lit; Bluetooth says the name a phone sees (LP-0b0c), Access edit · USB, Firmware its version, Hardware the model; the primary is Add a project (\"This board has no project yet. Add a project to play on it.\"). INSTALLED, SIGNED IN: lightplayer.app says the account and why it can't reach the board, river@example.com · Wi‑Fi not set up, not lit — its fix is the Wi‑Fi row's (Yona's phone, 10-19: \"River's account\" there read as \"this will work\"). SET UP, SIGNED IN: lightplayer.app river@example.com · online. A BLANK CHIP: only Firmware none · Install is lit, and Install firmware is the primary; the radio rows say — until there is firmware to ask. SET UP: nothing is lit, every row says what it reaches. OFFLINE: every row keeps its last value, dimmed; Bluetooth says on, not a name — the name follows what the board runs, and offline that is not known. EMULATED: the USB row says what it is, the radio rows —."
)]
fn board_card_setup_rows() -> Element {
    rsx! {
        section { class: GRID_CLASS,
            Cell { caption: "installed · signed out",
                StoryBoardCard {
                    card: fresh(),
                    plays: BoardPlays::Nothing,
                    wifi: Some(nothing_saved()),
                    access: Some(usb_access(Some(true), false)),
                    on_action: |_| {},
                }
            }
            Cell { caption: "installed · signed in",
                StoryBoardCard {
                    card: fresh(),
                    plays: BoardPlays::Nothing,
                    wifi: Some(nothing_saved()),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "a blank chip",
                StoryBoardCard {
                    card: needs_firmware(lpa_studio_core::DeviceFirmwareFace::Blank),
                    on_action: |_| {},
                }
            }
            Cell { caption: "set up · signed in",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(joined(RelayState::Connected)),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "offline · last known",
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    remembered_network: joined(RelayState::Connected).status,
                    remembered_bluetooth: Some(true),
                    account: Some(ACCOUNT.to_string()),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * DAY),
                    on_action: |_| {},
                }
            }
            Cell { caption: "emulated",
                StoryBoardCard {
                    card: emulated(),
                    feed: live_feed(),
                    runtime: Some(UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.5))),
                    plays: running(),
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Every state of the link rows --------------------------------------------

#[story(
    description = "The four link rows in their states (AC2): each row's kind is the link's own name, and the row says what that link reaches or how it stands — the fact; the button is the fix. WRONG PASSWORD: Wi‑Fi Cedar Lane · wrong password · Change, orange. JOINING: the spinner in the icon's place, Cedar Lane · joining. SAVED, NOT IN RANGE: Cedar Lane · not connected, quiet. ON WI‑FI: this tab's link is the LAN, so Wi‑Fi says Cedar Lane · connected and USB not connected. OVER BLUETOOTH: Bluetooth LP-aurora-drift · connected. CAN'T REACH: joined, and lightplayer.app didn't answer: river@example.com · can't reach, orange. ALL DISABLED: Wi‑Fi, Bluetooth and lightplayer.app hushed from their details say disabled, plain, with no button. THROUGH LIGHTPLAYER.APP: lightplayer.app river@example.com · connected, the cloud icon."
)]
fn board_card_link_row_states() -> Element {
    rsx! {
        section { class: GRID_CLASS,
            Cell { caption: "Wi‑Fi wrong password",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(wifi(
                        true,
                        true,
                        StationState::Failed {
                            ssid: HOME.to_string(),
                            reason: StationFailure::WrongPassword,
                        },
                        vec![saved(HOME)],
                        RelayState::WaitingForInternet,
                    )),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "Wi‑Fi joining",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(wifi(
                        true,
                        true,
                        StationState::Connecting {
                            ssid: HOME.to_string(),
                            step: lpc_wire::ConnectStep::CheckingPassword,
                        },
                        vec![saved(HOME)],
                        RelayState::WaitingForInternet,
                    )),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "saved, not in range",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(wifi(
                        true,
                        true,
                        StationState::NotConnected,
                        vec![saved(HOME)],
                        RelayState::WaitingForInternet,
                    )),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "on Wi‑Fi",
                StoryBoardCard {
                    card: over_network(porch()),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    link: Some(UiLinkKind::Wifi),
                    lan: lan_link_for_endpoint("lan:ws://192.168.1.42/link"),
                    wifi: Some(joined(RelayState::Connected)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "over Bluetooth",
                StoryBoardCard {
                    card: over_network(porch()),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    link: Some(UiLinkKind::Bluetooth),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "can't reach lightplayer.app",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(joined(RelayState::WaitingForInternet)),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "all three disabled",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(wifi(
                        false,
                        false,
                        StationState::Off,
                        vec![saved(HOME)],
                        RelayState::Off,
                    )),
                    access: Some(usb_access(Some(false), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "through lightplayer.app",
                StoryBoardCard {
                    card: over_network(porch()),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    link: Some(UiLinkKind::Relay),
                    lan: lan_link_for_endpoint("relay:6055f90a0b0c"),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Each radio row's own details --------------------------------------------

#[story(
    description = "The Wi‑Fi row's details on a new board (AC3, AC5): the switch line first — Wi‑Fi and the board's own switch, on — as macOS draws the top of its Wi‑Fi menu; then Set up's Wi‑Fi panel, nothing saved, so it opens on what the board hears. Hushing the row is flipping that switch off. The panel draws no heading or switches of its own on the card: the switch line is both."
)]
fn board_card_setup_wifi_details() -> Element {
    let mut wifi = nothing_saved();
    wifi.heard = Some(vec![
        lpa_studio_core::HeardNetwork {
            ssid: HOME.to_string(),
            rssi: -48,
            secure: true,
        },
        lpa_studio_core::HeardNetwork {
            ssid: "Cedar Lane Guest".to_string(),
            rssi: -63,
            secure: false,
        },
    ]);
    details_story(rsx! {
        StoryBoardCard {
            card: fresh(),
            plays: BoardPlays::Nothing,
            wifi: Some(wifi),
            access: Some(usb_access(Some(true), false)),
            details_open: Some(CardPart::Bar(BarLayer::Wifi)),
            on_action: |_| {},
        }
    })
}

#[story(
    description = "The lightplayer.app row's details, signed out: the switch line first (lightplayer.app and the board's Cloud relay switch, on), then what signing in gets you (\"Sign in to reach this board from any browser, through lightplayer.app.\") with Sign in — the person's own click, it leaves the page — and that nothing goes to lightplayer.app until you sign in."
)]
fn board_card_setup_cloud_details() -> Element {
    details_story(rsx! {
        StoryBoardCard {
            card: fresh(),
            plays: BoardPlays::Nothing,
            wifi: Some(nothing_saved()),
            access: Some(usb_access(Some(true), false)),
            details_open: Some(CardPart::Bar(BarLayer::Cloud)),
            on_action: |_| {},
        }
    })
}

#[story(
    description = "The Bluetooth row's details on an open board: the switch line first (Bluetooth and the board's switch, on; its hover says a flip restarts the board), then the name a phone sees, what it is for, and that anyone nearby can connect and edit it with no password yet."
)]
fn board_card_setup_bluetooth_details() -> Element {
    details_story(rsx! {
        StoryBoardCard {
            card: fresh(),
            plays: BoardPlays::Nothing,
            wifi: Some(nothing_saved()),
            access: Some(usb_access(Some(true), false)),
            details_open: Some(CardPart::Bar(BarLayer::Bluetooth)),
            on_action: |_| {},
        }
    })
}

#[story(
    description = "A hushed row's details: Wi‑Fi disabled — its switch line stands off over \"Wi‑Fi is disabled on this board. It reaches Studio over USB and Bluetooth only.\"; flipping it on is the way back. The row itself says disabled, plain, and wants nothing."
)]
fn board_card_setup_disabled_details() -> Element {
    details_story(rsx! {
        StoryBoardCard {
            card: porch(),
            feed: live_feed(),
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            wifi: Some(wifi(
                false,
                true,
                StationState::Off,
                vec![saved(HOME)],
                RelayState::Off,
            )),
            access: Some(usb_access(Some(true), false)),
            account: Some(ACCOUNT.to_string()),
            details_open: Some(CardPart::Bar(BarLayer::Wifi)),
            on_action: |_| {},
        }
    })
}

// --- The card at a phone's two-up --------------------------------------------

#[story(
    description = "The eight rows at a phone's two-up width (the home grid's two columns on a 375 px screen): the picture drops to 108 px, the kind column narrows, a link's longer name is cut at its end and a value too long beside its button is cut too — the row's hover and its details hold them whole. Left, a new board signed out with its lit rows; right, a set-up board."
)]
fn board_card_setup_rows_two_up() -> Element {
    rsx! {
        section { class: "tw:w-[375px] tw:p-4",
            div { class: "tw:grid tw:grid-cols-2 tw:gap-2",
                StoryBoardCard {
                    card: fresh(),
                    plays: BoardPlays::Nothing,
                    wifi: Some(nothing_saved()),
                    access: Some(usb_access(Some(true), false)),
                    on_action: |_| {},
                }
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(joined(RelayState::Connected)),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- The name, renamed where it is --------------------------------------------

#[story(
    description = "Rename where the name is (AC7): the name bar's name with a faint pencil after it, always shown (brighter, and the name underlined, on hover); pressed, a field takes the name's place at the bar's height with the text selected — Enter or clicking away keeps it, Esc puts it back. Hardware's details no longer hold a name field."
)]
fn board_card_rename_in_place() -> Element {
    rsx! {
        section { class: GRID_CLASS,
            Cell { caption: "the name, pressable",
                StoryBoardCard {
                    card: fresh(),
                    plays: BoardPlays::Nothing,
                    on_action: |_| {},
                }
            }
            Cell { caption: "renaming",
                StoryBoardCard {
                    card: fresh(),
                    plays: BoardPlays::Nothing,
                    previews: CardPreviews {
                        renaming: true,
                        ..CardPreviews::default()
                    },
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Fixtures --------------------------------------------------------------------

/// The made-up account, as lightplayer.app's row names it: its email.
const ACCOUNT: &str = "river@example.com";

/// The made-up home network.
const HOME: &str = "Cedar Lane";

/// A board just installed: LightPlayer on it, nothing played, named after
/// its model.
fn fresh() -> DeviceView {
    DeviceView {
        title: "XIAO ESP32-C6".to_string(),
        loaded_project: DeviceLoadedProject::Empty,
        can_remove_project: false,
        engine_fps: None,
        terminal: Vec::new(),
        ..porch()
    }
}

/// On, Cloud relay on, nothing saved: a new board's Wi‑Fi.
fn nothing_saved() -> UiDeviceWifi {
    wifi(
        true,
        true,
        StationState::NotConnected,
        Vec::new(),
        RelayState::NoAccount,
    )
}

/// Joined to the home network, lightplayer.app at `relay`.
fn joined(relay: RelayState) -> UiDeviceWifi {
    wifi(
        true,
        true,
        StationState::Connected {
            ssid: HOME.to_string(),
            ip: "192.168.1.42".to_string(),
            rssi: -48,
            host: "lp-0b0c.local".to_string(),
        },
        vec![saved(HOME)],
        relay,
    )
}

/// The board's Wi‑Fi as it reports it.
fn wifi(
    on: bool,
    cloud_relay: bool,
    station: StationState,
    networks: Vec<SavedNetworkInfo>,
    relay: RelayState,
) -> UiDeviceWifi {
    UiDeviceWifi {
        status: Some(NetworkStatus {
            wifi: on,
            cloud_relay,
            networks,
            station,
            relay,
        }),
        ..UiDeviceWifi::new(DeviceId(7), true)
    }
}

fn saved(ssid: &str) -> SavedNetworkInfo {
    SavedNetworkInfo {
        ssid: ssid.to_string(),
        has_password: true,
        hidden: false,
        last: None,
    }
}

/// A details story: one card with room under it for its open details.
fn details_story(card: Element) -> Element {
    rsx! {
        section { class: "tw:min-h-[900px] tw:p-4",
            div { class: "tw:w-[340px] tw:max-w-full", {card} }
        }
    }
}

/// One captioned card in a grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn Cell(caption: String, children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-1.5",
            p { class: CAPTION_CLASS, "{caption}" }
            {children}
        }
    }
}

/// The states grid: the home page's own (`device_grid_class`), as many
/// 320 px cards as fit.
const GRID_CLASS: &str =
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(320px,1fr))] tw:items-start tw:gap-3.5 tw:p-4";

/// A cell's caption.
const CAPTION_CLASS: &str =
    "tw:m-0 tw:text-[10.5px] tw:font-bold tw:uppercase tw:tracking-[0.06em] tw:text-dim-foreground";
