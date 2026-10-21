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
use lpa_studio_core::app::devices::cloud_board_picture::{
    CloudBoardPicture, cloud_picture_preview,
};
use lpa_studio_core::app::devices::ui_cloud_board::ONLINE_AT_LIGHTPLAYER_APP;
use lpa_studio_core::{
    BarLayer, BoardPlays, DeviceId, DeviceLoadedProject, DeviceView, NetworkStatus,
    SavedNetworkInfo, StationState, UiCloudBoard, UiDeviceWifi, UiLinkKind, UiRuntimeBand,
    lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;
use lpc_wire::RelayState;
use lpc_wire::server::StationFailure;

use super::board_card_stories::{
    DAY, HOUR, PORCH_UID, emulated, last_picture, live_feed, needs_firmware, offline, over_network,
    porch, running,
};
use super::{CardPart, CardPreviews};
use crate::app::home::ble_access_stories::usb_access;
use crate::app::home::device_offer_story_fixtures::{
    STORY_BOARD_NOW, StoryBoardCard, story_board_prefix,
};

// --- A new board's first minutes --------------------------------------------

#[story(
    description = "A new board's setup is its card (AC4, AC6): the rows with something left to do are LIT — the whole row on a lighter flat ground, its icon, kind and name at full strength, its button on the same ground with no fill of its own; no gradient, no dot. INSTALLED, SIGNED OUT: Project none · Add, Wi‑Fi not connected · Set up, lightplayer.app not connected · Sign in are lit; Bluetooth says the name a phone sees (LP-0b0c), Access edit · USB, Firmware its version, Hardware the model; the primary is Add a project (\"This board has no project yet. Add a project to play on it.\"). INSTALLED, SIGNED IN: lightplayer.app says the account and why it can't reach the board, river@example.com · no Wi‑Fi, not lit — its fix is the Wi‑Fi row's (Yona's phone, 10-19: \"River's account\" there read as \"this will work\"). SET UP, SIGNED IN: lightplayer.app river@example.com · online. A BLANK CHIP: only Firmware none · Install is lit, and Install firmware is the primary; the radio rows say — until there is firmware to ask. SET UP: nothing is lit, every row says what it reaches. OFFLINE: every row keeps its last value, dimmed; Bluetooth says on, not a name — the name follows what the board runs, and offline that is not known. EMULATED: the USB row says what it is, the radio rows —."
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
    description = "A board online through lightplayer.app (your boards on every computer, M1): the signed-in account's list (ListAccountBoards) says it is online, and no link of this tab reaches it. NEVER MET HERE: set up on a phone (Yona's \"Fabric Test\", 10-19), it is in Online boards all the same — its surface is its picture from lightplayer.app, one row of lamps per output, not dimmed and with no words; Connect, the cloud icon, is the primary; lightplayer.app says river@example.com · online; Project says what it told lightplayer.app it plays, Firmware its version; the radio rows say — (nothing here has heard it); Connect is its only verb (no rename: nothing here can reach it yet). MET HERE: a board this browser remembers, unplugged, moves to Online boards the same way: lightplayer.app's picture over its saved one, its rows its last facts dimmed, lightplayer.app online and not dimmed. NO PICTURE (relay protocol 1, or none yet): the dark, no words, Connect — nothing is said about the missing picture."
)]
fn board_card_online_through_lightplayer_app() -> Element {
    rsx! {
        section { class: GRID_CLASS,
            Cell { caption: "never met here",
                StoryBoardCard {
                    card: never_met(),
                    never_met: true,
                    link: Some(UiLinkKind::Relay),
                    relay: true,
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(cloud_board(true)),
                    on_action: |_| {},
                }
            }
            Cell { caption: "met here · unplugged",
                StoryBoardCard {
                    card: DeviceView {
                        state_label: ONLINE_AT_LIGHTPLAYER_APP.to_string(),
                        ..offline()
                    },
                    feed: last_picture(),
                    open_uid: Some(PORCH_UID.to_string()),
                    relay: true,
                    remembered_network: joined(RelayState::Connected).status,
                    remembered_bluetooth: Some(true),
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(cloud_board(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * DAY),
                    on_action: |_| {},
                }
            }
            Cell { caption: "no picture",
                StoryBoardCard {
                    card: never_met(),
                    never_met: true,
                    link: Some(UiLinkKind::Relay),
                    relay: true,
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(cloud_board(false)),
                    on_action: |_| {},
                }
            }
        }
    }
}

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

// --- The account's board list (M2) ----------------------------------------------

#[story(
    description = "A board on your account, offline everywhere (the account's board list, M2): signed in on a computer that never met it, it is in Offline boards all the same. NEVER MET HERE, ITS LAST PICTURE: the last picture lightplayer.app kept of it, dimmed under \"This board is offline. It was last seen 2 hours ago.\"; lightplayer.app says river@example.com · 2 h ago, dimmed (the age alone: a state beside an email stays short); Project and Firmware its last known, dimmed; Hardware its model, from the Studio that reported it; no Connect (nothing reaches it), and Remove from your account is its one verb. NEVER MET HERE, NO PICTURE: the same on the dark. MET HERE: a board this browser remembers keeps its own saved picture; the account's record only freshens its facts and its lightplayer.app row."
)]
fn board_card_offline_on_your_account() -> Element {
    rsx! {
        section { class: GRID_CLASS,
            Cell { caption: "never met here · its last picture",
                StoryBoardCard {
                    card: never_met_offline(),
                    never_met: true,
                    on_account: true,
                    link: Some(UiLinkKind::Relay),
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(kept_cloud_board(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * HOUR),
                    on_action: |_| {},
                }
            }
            Cell { caption: "never met here · no picture",
                StoryBoardCard {
                    card: never_met_offline(),
                    never_met: true,
                    on_account: true,
                    link: Some(UiLinkKind::Relay),
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(kept_cloud_board(false)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * HOUR),
                    on_action: |_| {},
                }
            }
            Cell { caption: "met here · unplugged",
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    open_uid: Some(PORCH_UID.to_string()),
                    relay: true,
                    on_account: true,
                    remembered_network: joined(RelayState::Connected).status,
                    remembered_bluetooth: Some(true),
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(kept_cloud_board(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * HOUR),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Remove from your account, armed in the open Hardware details (the account's board list, M2, D10): the danger zone holds Factory reset, Forget on this browser, then Remove from your account — Lasting, two clicks, and the agent never presses it; its words say the board leaves your account's list on every computer, keeps its keys, and comes back the next time it reaches lightplayer.app."
)]
fn board_card_remove_from_your_account_armed() -> Element {
    let remove = story_board_prefix(offline().id).child("forget-from-account");
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    open_uid: Some(PORCH_UID.to_string()),
                    on_account: true,
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(kept_cloud_board(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * HOUR),
                    details_open: Some(CardPart::Bar(BarLayer::Hardware)),
                    armed_preview: Some(remove),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Forget on this browser, armed, on a board your account holds (M2, D10): the card's Forget is this browser's only — the board still shows from your account on every computer, and its words say so; Remove from your account, beside it, is the account's."
)]
fn board_card_forget_on_this_browser_on_your_account() -> Element {
    let forget = story_board_prefix(offline().id).child("forget");
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    open_uid: Some(PORCH_UID.to_string()),
                    on_account: true,
                    account: Some(ACCOUNT.to_string()),
                    cloud: Some(kept_cloud_board(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 2.0 * HOUR),
                    details_open: Some(CardPart::Bar(BarLayer::Hardware)),
                    armed_preview: Some(forget),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "A full list (M2, D3): the board holds your account's key, and your account already holds 64 boards, so lightplayer.app did not add it. lightplayer.app says river@example.com · list full, orange; its details say \"Your account holds 64 boards. Remove one to add this one.\" — once, with no button of its own: a board leaves the list from its own card."
)]
fn board_card_account_list_full() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(joined(RelayState::Connected)),
                    access: Some(usb_access(Some(true), false)),
                    account: Some(ACCOUNT.to_string()),
                    account_note: Some(lpa_studio_core::AccountBoardNote::ListFull { max: 64 }),
                    details_open: Some(CardPart::Bar(BarLayer::Cloud)),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// A details card's frame, and the card's width in it.
const DETAILS_FRAME_CLASS: &str = "tw:min-h-[900px] tw:p-4";
const CARD_WIDTH_CLASS: &str = "tw:w-[300px] tw:max-w-full";

// --- Fixtures --------------------------------------------------------------------

/// A board this browser never met, as core's roster holds it from the
/// account's list: offline here, named by the row, its firmware and
/// project as it told lightplayer.app.
fn never_met() -> DeviceView {
    DeviceView {
        id: DeviceId(11),
        title: "Fabric Test".to_string(),
        state_label: ONLINE_AT_LIGHTPLAYER_APP.to_string(),
        board_id: None,
        detected_chip: None,
        identity_label: None,
        remembered_firmware: Some("2026.10.19-1".to_string()),
        loaded_project: DeviceLoadedProject::Running {
            label: "rainbow".to_string(),
        },
        escapes: Vec::new(),
        ..offline()
    }
}

/// The account's row for "Fabric Test", with its picture from
/// lightplayer.app (two outputs, a rainbow across them) or without one.
fn cloud_board(picture: bool) -> UiCloudBoard {
    let samples = 48usize;
    let colors: Vec<u8> = (0..samples)
        .flat_map(|sample| rainbow(sample as f32 / samples as f32))
        .collect();
    let shown = CloudBoardPicture {
        seq: 7,
        at: STORY_BOARD_NOW - 0.4,
        outputs: vec![30, 18],
        colors,
    };
    UiCloudBoard {
        key: lpa_studio_core::BoardKey::parse("10bda3b08e30").expect("a MAC"),
        label: "Fabric Test".to_string(),
        model: None,
        firmware: Some("2026.10.19-1".to_string()),
        project: Some("rainbow".to_string()),
        online: true,
        last_seen: Some(STORY_BOARD_NOW - 0.4),
        relay_proto: if picture { 2 } else { 1 },
        picture: picture.then(|| cloud_picture_preview(&shown)).flatten(),
        picture_at: picture.then_some(shown.at),
    }
}

/// The account's record of "Porch", offline everywhere, last seen two hours
/// ago, with the last picture lightplayer.app kept of it or without one.
fn kept_cloud_board(picture: bool) -> UiCloudBoard {
    let samples = 40usize;
    let colors: Vec<u8> = (0..samples)
        .flat_map(|sample| rainbow(0.55 + 0.2 * sample as f32 / samples as f32))
        .collect();
    let kept = CloudBoardPicture {
        seq: 1_760_000_000_000,
        at: STORY_BOARD_NOW - 2.0 * HOUR,
        outputs: vec![40],
        colors,
    };
    UiCloudBoard {
        key: lpa_studio_core::BoardKey::parse("10bda3b08e31").expect("a MAC"),
        label: "Porch".to_string(),
        model: Some("seeed/xiao-esp32-c6".to_string()),
        firmware: Some("2026.10.17-2".to_string()),
        project: Some("rocaille".to_string()),
        online: false,
        last_seen: Some(STORY_BOARD_NOW - 2.0 * HOUR),
        relay_proto: 0,
        picture: picture.then(|| cloud_picture_preview(&kept)).flatten(),
        picture_at: picture.then_some(kept.at),
    }
}

/// A board this browser never met, on the account's list and offline
/// everywhere, as core's roster holds it: named by the record, its model,
/// firmware and project the last a Studio or the relay heard.
fn never_met_offline() -> DeviceView {
    DeviceView {
        id: DeviceId(12),
        title: "Porch".to_string(),
        state_label: "Offline".to_string(),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        remembered_firmware: Some("2026.10.17-2".to_string()),
        loaded_project: DeviceLoadedProject::Running {
            label: "rocaille".to_string(),
        },
        ..never_met()
    }
}

/// A hue around the wheel, as sRGB codes.
fn rainbow(hue: f32) -> [u8; 3] {
    let channel = |offset: f32| {
        let x = ((hue + offset) * 6.0).rem_euclid(6.0);
        let level = (x - 3.0).abs() - 1.0;
        (level.clamp(0.0, 1.0) * 255.0) as u8
    };
    [channel(0.0), channel(2.0 / 3.0), channel(1.0 / 3.0)]
}

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
