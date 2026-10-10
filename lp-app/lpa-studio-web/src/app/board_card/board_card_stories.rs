//! The board card's stories: every state in one row (the card's one height
//! is measured here, AC5), each bar's details open, the corner's, a tall
//! details card over its own bar, the narrow card — and today's panels in
//! the details they moved to (access, Wi‑Fi, Bluetooth, link counters,
//! other version, the layout question and its refusal, rename, a Lasting
//! verb armed in place, the terminal).
//!
//! The details stories leave room under the card, but a details card taller
//! than the capture's 760 px viewport is placed where the shared popover
//! puts it on a short page — across its own bar, with the bar's copy not
//! drawn — exactly as it would be for a person on one.
//!
//! Every card is built by core's own builder ([`StoryBoardCard`] calls
//! `board_card` over the tree core would publish), so a story card says and
//! offers exactly what core decides for its board.

use dioxus::prelude::*;
use lpa_studio_core::app::devices::device_layout_step::LayoutStaging;
use lpa_studio_core::{
    ActivityEnd, BarLayer, BoardConnection, BoardPlays, DeviceActivityKind, DeviceActivityView,
    DeviceBoardFs, DeviceCardFeedView, DeviceEscape, DeviceFirmwareAge, DeviceFirmwareFace,
    DeviceFlashLayoutView, DeviceFlashStep, DeviceId, DeviceLayoutVerdict, DeviceLinkCounters,
    DeviceLinkId, DeviceLoadedProject, DeviceStatus, DeviceTerminalKind, DeviceTerminalLine,
    DeviceView, DeviceWireVersion, FIRMWARE_NEEDS_USB, FeedLiveness, HeardNetwork, HeldElsewhere,
    HoldLevel, HoldVia, INSTALL_FIND_PARAM, LastAttempt, NetworkStatus, OfferArgs, OutcomeView,
    PendingLinkView, SavedNetworkInfo, StationState, UiDeviceAccess, UiDeviceWifi, UiLinkKind,
    UiOfferTree, UiPanelGroup, UiRuntimeBand, UiTakeOver, UiUnlockOffer, UiWifiConnect,
    UpdateFixture, UpdateFixtureRow, WIFI_BUSY_WORDS, board_panel_picks, device_layout_view,
    lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;
use lpc_wire::RelayState;

use super::other_version_form::OfferPickerPreview;
use super::{CardPart, CardPreviews};
use crate::app::home::ble_access_stories::usb_access;
use crate::app::home::device_offer_story_fixtures::{
    STORY_BOARD_NOW, StoryBoardCard, StoryNewBoardCard, story_board_prefix,
};
use crate::app::home::home_gallery_stories::live_card_lamp_frame;
use crate::app::module::module_fixtures::{
    card_few_controls_panel, card_many_controls_panel, card_root_panel, card_root_panel_held,
};

// --- Every state ------------------------------------------------------------

#[story(
    description = "The board card in every state, one height each (AC5): live (\"58 fps\" beside the blue dot, Connect as the primary, Edit on the project bar), connected (the board's panel in the five bars' place at their height: the master fader, three knobs, All controls · 2 more with Edit at its end; Done as the primary), connecting (the connection bar working: spinner, \"Identifying…\", the iridescent sweep along its foot, Cancel), offline with its last picture (dimmed, its age \"5 h ago\" in the corner, \"Offline · 2 weeks\" in its connection bar, Connect over Wi‑Fi), locked (over Bluetooth: Unlock, the picture dark), updating (the firmware bar's work at 40 % and the board's own light strip in the picture), done (the project bar green for a few seconds), failed (striped, Retry), a new board (Install with the board pick), an emulated board (\"Emulated XIAO ESP32-C6\", in this tab), a board reached through lightplayer.app (\"Wi‑Fi via lightplayer.app · live\", the cloud icon), and the three states of a board another tab of this browser holds: held (\"Open in another tab\" in orange, the picture that tab saved, dimmed, Connect), taking over (the connection bar working: \"Asking the other tab…\", Connect disabled) and taken (\"Taken by another tab\", Connect). The picture is 138 px, the name bar 50 px, each bar 28 px: every card is 330 px tall."
)]
fn board_card_every_state() -> Element {
    rsx! {
        section { class: STORY_GRID_CLASS,
            Cell { caption: "live",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    on_action: |_| {},
                }
            }
            Cell { caption: "connected",
                {connected_card(card_root_panel(), None)}
            }
            Cell { caption: "connecting",
                StoryBoardCard { card: identifying(), on_action: |_| {} }
            }
            Cell { caption: "offline, its last picture",
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    wifi_address: Some("192.168.1.40".to_string()),
                    last_seen_at: Some(STORY_BOARD_NOW - 14.0 * DAY),
                    on_action: |_| {},
                }
            }
            Cell { caption: "locked",
                StoryBoardCard {
                    card: over_network(porch()),
                    link: Some(UiLinkKind::Bluetooth),
                    access: Some(locked_access()),
                    on_action: |_| {},
                }
            }
            Cell { caption: "updating",
                {update_card(UpdateFixtureRow::Updating, None)}
            }
            Cell { caption: "done",
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    ended: Some(ActivityEnd {
                        kind: DeviceActivityKind::Push,
                        ok: true,
                        at: STORY_BOARD_NOW - 1.0,
                    }),
                    on_action: |_| {},
                }
            }
            Cell { caption: "failed",
                {failed_card(None)}
            }
            Cell { caption: "new",
                StoryNewBoardCard { pending: new_board(), on_action: |_| {} }
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
            Cell { caption: "through lightplayer.app",
                {relay_card(None)}
            }
            Cell { caption: "held by another tab",
                {held_card(HoldLevel::Watching, false, None)}
            }
            Cell { caption: "taking over",
                {held_card(HoldLevel::Watching, false, Some(asking_the_other_tab()))}
            }
            Cell { caption: "taken by another tab",
                {held_card(HoldLevel::Watching, true, None)}
            }
        }
    }
}

// --- A board another tab holds ----------------------------------------------

#[story(
    description = "A board another tab of this browser holds (one tab holds each board; the others show its last picture and say so). Three frames of the same card, by what the holder is doing: WATCHING (it holds the board and nothing of the user's is open there): the picture that tab saved, dimmed, its age (\"5 min ago\") in the corner; the connection bar says \"Open in another tab\" in orange, the colour of \"someone has it\", and nothing else; the name bar's primary is Connect, whose offer is the take-over; the corner is orange too, its details saying \"The last picture another tab saved.\". EDITOR OPEN: the aside says \"editor open\", and Connect wears the error tint (taking the board closes that editor; the undo is pressing Connect over there). BUSY: the aside is what the holder is doing (\"Updating · 42%\"), and Connect is disabled saying \"Busy in the other tab: Updating · 42%\". The card is the board's own card: same height, same five bars."
)]
fn board_card_held_by_another_tab() -> Element {
    rsx! {
        section { class: STORY_GRID_CLASS,
            Cell { caption: "watching",
                {held_card(HoldLevel::Watching, false, None)}
            }
            Cell { caption: "its editor open",
                {held_card(HoldLevel::Open, false, None)}
            }
            Cell { caption: "busy",
                {held_card(HoldLevel::Busy("Updating · 42%".to_string()), false, None)}
            }
        }
    }
}

#[story(
    description = "Connect on a board another tab holds, while it runs and when it fails. ASKING: the connection bar is the work (the spinner and \"Asking the other tab…\", the iridescent sweep along its foot), Connect is disabled saying the same words, and the summary under it is unchanged. OPENING: the other tab let go; \"Opening…\" while the board opens here. FAILED: the bar is striped with \"That tab didn't answer\" (the holder may be a tab of an older build) and Retry, which presses the same take-over; Connect is a press again. The picture and the status corner do not change for work."
)]
fn board_card_taking_over() -> Element {
    rsx! {
        section { class: STORY_GRID_CLASS,
            Cell { caption: "asking the other tab",
                {held_card(HoldLevel::Watching, false, Some(asking_the_other_tab()))}
            }
            Cell { caption: "opening",
                {held_card(HoldLevel::Watching, false, Some(UiTakeOver {
                    words: "Opening…".to_string(),
                    failed: false,
                }))}
            }
            Cell { caption: "that tab didn't answer",
                {held_card(HoldLevel::Watching, false, Some(UiTakeOver {
                    words: "That tab didn't answer".to_string(),
                    failed: true,
                }))}
            }
        }
    }
}

#[story(
    description = "The tab that let go because another tab asked. Its card stays: \"Taken by another tab\" in orange on the connection bar, the picture it saved on the way out (dimmed, with its age), and Connect, which takes the board back the same way. Nothing reopens by itself."
)]
fn board_card_taken_from_this_tab() -> Element {
    rsx! {
        section { class: STORY_GRID_CLASS,
            Cell { caption: "taken by another tab",
                {held_card(HoldLevel::Watching, true, None)}
            }
            Cell { caption: "its editor open over there",
                {held_card(HoldLevel::Open, true, None)}
            }
        }
    }
}

#[story(
    description = "A board whose one network connection is someone else's — a person Studio cannot name, so there is no tab to ask. The connection bar says \"Someone else connected\" in orange, and offers Retry on the same road (Wi‑Fi, or the cloud) and nothing more: taking a board from another person is sharing's question. Compare \"Open in another tab\", which is a tab of this browser and whose Connect takes it over."
)]
fn board_card_network_busy() -> Element {
    rsx! {
        section { class: STORY_GRID_CLASS,
            Cell { caption: "over Wi‑Fi",
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    wifi_address: Some("192.168.1.40".to_string()),
                    wifi_connect: Some(busy_connect(false)),
                    last_seen_at: Some(STORY_BOARD_NOW - 14.0 * DAY),
                    on_action: |_| {},
                }
            }
            Cell { caption: "through lightplayer.app",
                StoryBoardCard {
                    card: offline(),
                    feed: last_picture(),
                    relay: true,
                    wifi_connect: Some(busy_connect(true)),
                    last_seen_at: Some(STORY_BOARD_NOW - 14.0 * DAY),
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Connected --------------------------------------------------------------

#[story(
    description = "A connected board (Connect pressed on its card): the five bars give way, at the same height, to the board's panel — the master brightness fader across the card (label, fader, value), then a row of three knobs (speed and hue following what drives them, in violet; palette, stepped, at its default), then \"All controls · 2 more\" (a link to the board's own play page; the mirror toggle and a second fader are there) with Edit flush at the row's end. Done is the primary; the picture is the session's own frames. Same widgets and gestures as the panel everywhere else: drag a control and it turns gold and offers to let go. No panel reset and no auto-save switch on the card (the play page has both)."
)]
fn board_card_connected() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                {connected_card(card_root_panel(), None)}
            }
        }
    }
}

#[story(
    description = "The connected card at a phone's width, beside a watched one: the picture is 108 px, and the panel keeps the five bars' height (140 px) — master fader, the three knobs, All controls · 2 more with Edit. Both cards are 300 px tall."
)]
fn board_card_connected_narrow() -> Element {
    rsx! {
        section { class: "tw:grid tw:w-[390px] tw:max-w-full tw:grid-cols-2 tw:gap-[9px] tw:p-3",
            {connected_card(card_root_panel(), None)}
            StoryBoardCard {
                card: porch(),
                feed: live_feed(),
                open_uid: Some(PORCH_UID.to_string()),
                plays: running(),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A connected board whose project has one control and no master: the row holds the one knob, and the All controls row has no \"more\" (the play page has nothing else), Edit at its end. The panel keeps the bars' height."
)]
fn board_card_connected_few_controls() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                {connected_card(card_few_controls_panel(), None)}
            }
        }
    }
}

#[story(
    description = "A connected board with a big panel (fifteen controls across the root and four effects): the card draws the master and three knobs, at most four, and \"All controls · 11 more\". The card does not grow."
)]
fn board_card_connected_many_controls() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                {connected_card(card_many_controls_panel(), None)}
            }
        }
    }
}

#[story(
    description = "A connected board with one control held (the hue knob, dragged on the card): gold, and its let-go glyph beside its label, exactly as on the panel everywhere else. Letting go returns it to following the project."
)]
fn board_card_connected_engaged() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                {connected_card(card_root_panel_held("hue", 0.82), None)}
            }
        }
    }
}

#[story(
    description = "A board connected over Bluetooth with the play password (\"Unlocked with friends · play\"): the panel works the same — every control writes — but there is no auto-save anywhere on the card, and the All controls row's Edit wears the lock (pressing it asks for the edit password). Emulated boards answer at the edit tier, so this state is in stories, not on a live walk."
)]
fn board_card_connected_play_only() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                {connected_card(card_root_panel(), Some(play_only_access()))}
            }
        }
    }
}

#[story(
    description = "Connect pressed: the board's session is opening (or the board is being reached first). The connection bar is the work — the spinner and \"Connecting…\", the iridescent sweep along its foot — and the primary reads \"Connecting…\", disabled (no Cancel: the open is bounded by its deadline). The bars stay until the panel is ready."
)]
fn board_card_connecting() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    connection: BoardConnection::Connecting,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The last Connect did not open: the connection bar is striped with \"Couldn't connect\" and Retry (Connect again), the reason in the bar's details. The card is otherwise the board's facts, Connect its primary."
)]
fn board_card_connect_failed() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    connection: BoardConnection::Failed {
                        reason: "The board didn't answer in time".to_string(),
                    },
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The board's card docked in the editor (Edit pressed on a connected card): the five bars, the session's picture, and Done as the primary — Done ends the session from here as it does from the home page. No Edit: the editor already shows the board."
)]
fn board_card_docked_done() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: lens_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    docked: true,
                    connection: BoardConnection::Connected,
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Each part's details, open ---------------------------------------------

#[story(
    description = "The project bar's details, open: Studio's detail card merged with the bar that opened it (the same shared popover, its trigger the bar's whole width). What the board plays, then its verbs as menu rows — Edit, Put another project on it (the project pick: one popover at a time, so it closes the details and opens the gallery over the bar) — and Remove project apart, behind the danger zone's red line, arming in place on two clicks."
)]
fn board_card_project_details_open() -> Element {
    details_open(BarLayer::Project)
}

#[story(
    description = "The connection bar's details, open: how the board is reached (the link, the last word heard, the board's own words), its links, then Identify again and Disconnect as menu rows."
)]
fn board_card_connection_details_open() -> Element {
    details_open(BarLayer::Connection)
}

#[story(
    description = "The access bar's details, open: what this link may do, and with what (USB)."
)]
fn board_card_access_details_open() -> Element {
    details_open(BarLayer::Access)
}

#[story(
    description = "The firmware bar's details, open: the version, the build, then the firmware verbs as menu rows, and Factory reset apart behind the danger zone's line."
)]
fn board_card_firmware_details_open() -> Element {
    details_open(BarLayer::Firmware)
}

#[story(
    description = "A new board's hardware details, open: its chip and its id once read, Reset, and Dismiss apart behind the danger zone's line — the ways out of a link still saying who it is."
)]
fn board_card_hardware_details_open() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryNewBoardCard {
                    pending: new_board(),
                    details_open: Some(CardPart::Bar(BarLayer::Hardware)),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The status corner's details, open from the notch: how the board is running (its state, its frame rate, the picture's words) and the board's terminal, flush in the card — what it said, typed and in order, the heartbeat collapsed."
)]
fn board_card_corner_details_open() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Corner),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The 2026-10-06 regression case: a details card taller than the room below its bar — the connection details with the Wi‑Fi panel's long Nearby list (nothing saved, so it opens on what the board hears) — opened from a card at the bottom of the page. The shared popover places the card where it fits across its own wide bar, and the bar's copy is never drawn over the card's rows (`docs/defects/2026-10-06-a-card-row-shows-through-its-own-popover.md`); past the viewport's budget the details scroll inside themselves."
)]
fn board_card_tall_details_over_its_bar() -> Element {
    let mut wifi = wifi_board(StationState::NotConnected, Vec::new());
    let mut heard = vec![
        heard_network("Starlink Home", -48, true),
        heard_network("NETGEAR42", -71, true),
        heard_network("xfinitywifi", -77, false),
    ];
    heard.extend((1..=10).map(|n| heard_network(&format!("Neighbour {n:02}"), -80 - n, true)));
    wifi.heard = Some(heard);
    rsx! {
        section { class: "tw:flex tw:min-h-[760px] tw:items-end tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(wifi),
                    details_open: Some(CardPart::Bar(BarLayer::Connection)),
                    on_action: |_| {},
                }
            }
        }
    }
}

// --- Today's panels, in their bars' details ---------------------------------

#[story(
    description = "The access bar's details over USB, with the access panel in them as its own sections (no box in a box): who can play and author, each Anyone or a password, and the keys list open — moved here from the Connections group's Access row."
)]
fn board_card_access_details_keys_open() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    access: Some(usb_access(Some(true), false)),
                    details_open: Some(CardPart::Bar(BarLayer::Access)),
                    previews: CardPreviews {
                        access_keys_open: true,
                        ..CardPreviews::default()
                    },
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The connection bar's details with the Wi‑Fi panel inline, as a section of the card (its divider, no frame): the connected network first, the other saved ones with their words, \"+ Connect to a network\", the Wi‑Fi and Cloud relay switches. Opening these details asks the board for its networks again, as opening today's Wi‑Fi popover did."
)]
fn board_card_connection_details_wifi() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    wifi: Some(connected_truck()),
                    details_open: Some(CardPart::Bar(BarLayer::Connection)),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The connection bar's details over USB just after Bluetooth was flipped: the switch row (moved from the Connections group), locked while it applies, and core's note under it — \"Restarting to turn Bluetooth on…\"."
)]
fn board_card_connection_details_bluetooth_restarting() -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    access: Some(usb_access(Some(true), true)),
                    details_open: Some(CardPart::Bar(BarLayer::Connection)),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The connection bar's details with the board's link counters (moved from today's ⋯ menu): resends, damaged frames, restarts, stalls and traffic in core's words and units, the two the link had to recover from in the warning tone, and core's caption."
)]
fn board_card_connection_details_link_counters() -> Element {
    let mut card = porch();
    card.link_counters = Some(DeviceLinkCounters {
        resends: 3,
        damaged: 0,
        resets: 1,
        stalls: 0,
        bytes_sent: 1_363_149,
        bytes_received: 38_912,
        frames_sent: 40,
        frames_received: 300,
    });
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card,
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Bar(BarLayer::Connection)),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The firmware bar's details with \"Other version…\" inline (no popover in a popover): the version box and the store's releases, an older version typed whole and picked, what installing it changes in core's words above the press, and the press armed — \"Confirm install\", two clicks — with From a file… beside it."
)]
fn board_card_firmware_details_other_version() -> Element {
    let fixture = UpdateFixture::new(UpdateFixtureRow::UpToDate, porch());
    rsx! {
        section { class: "tw:min-h-[1040px] tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: fixture.view.clone(),
                    update: fixture.words(),
                    update_facts: fixture.offer_facts(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Bar(BarLayer::Firmware)),
                    previews: CardPreviews {
                        other_version: Some(OfferPickerPreview {
                            args: OfferArgs::new().with(INSTALL_FIND_PARAM, "2026.10.05-1"),
                            armed: true,
                        }),
                        ..CardPreviews::default()
                    },
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The layout question, raised: an update waits to move the board's files to the new layout, so core raises the firmware bar's details and they open by themselves, the question in them — core's title and body, Download backup, Cancel, and Continue (one press: the panel is the question). No overlay any more; the card keeps its height."
)]
fn board_card_firmware_layout_question() -> Element {
    layout_card(false)
}

#[story(
    description = "The layout refusal, raised: the files do not fit the new firmware, nothing was changed, and the firmware details say so with Download backup and Close (nothing is running to cancel). Close closes the details, and they stay closed until core's refusal changes."
)]
fn board_card_firmware_layout_refused() -> Element {
    layout_card(true)
}

#[story(
    description = "The hardware bar's details with Rename (moved from today's ⋯ menu): the name field prefilled with what the board is called now, and Rename pressing the board's own rename offer."
)]
fn board_card_hardware_details_rename() -> Element {
    details_open(BarLayer::Hardware)
}

#[story(
    description = "Forget, armed in place inside the open hardware details: the danger zone's row reads \"Confirm Forget\" in the error tint with its quiet drain — two clicks, the way every Lasting verb arms. Nothing else on the card dims (the old card-wide marking is gone)."
)]
fn board_card_hardware_details_forget_armed() -> Element {
    let forget = story_board_prefix(porch().id).child("forget");
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Bar(BarLayer::Hardware)),
                    armed_preview: Some(forget),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The status corner's details with a longer terminal: lines fell off the front (the count first), a fault the board printed, Studio's own retry — all flush in the corner's details at a fixed height, scrolling inside it."
)]
fn board_card_corner_details_terminal() -> Element {
    let mut card = porch();
    card.terminal.extend([
        terminal_line(
            DeviceTerminalKind::Failure,
            "push failed: the board did not answer in 5 s",
        ),
        terminal_line(DeviceTerminalKind::Recovery, "Retrying the push"),
        terminal_line(DeviceTerminalKind::Outcome, "Pushed aurora-drift in 1.2 s"),
    ]);
    card.terminal_dropped = 42;
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card,
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Corner),
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The card at a phone's width, two to a row as the spike's phone frame has them: the card is narrow, so its picture is 108 px (the card asks its own width, a container query, not the page's); the name bar and the bars keep their heights, so every narrow card is 300 px tall."
)]
fn board_card_narrow() -> Element {
    rsx! {
        section { class: "tw:grid tw:w-[390px] tw:max-w-full tw:grid-cols-2 tw:gap-[9px] tw:p-3",
            StoryBoardCard {
                card: porch(),
                feed: live_feed(),
                open_uid: Some(PORCH_UID.to_string()),
                plays: running(),
                on_action: |_| {},
            }
            StoryBoardCard {
                card: offline(),
                feed: last_picture(),
                wifi_address: Some("192.168.1.40".to_string()),
                last_seen_at: Some(STORY_BOARD_NOW - 14.0 * DAY),
                on_action: |_| {},
            }
            {update_card(UpdateFixtureRow::Updating, None)}
            StoryNewBoardCard { pending: new_board(), on_action: |_| {} }
        }
    }
}

// --- Helpers ----------------------------------------------------------------

/// The live porch board with `layer`'s details open, room below for them.
fn details_open(layer: BarLayer) -> Element {
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    details_open: Some(CardPart::Bar(layer)),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// One cell of the states row: its caption over its card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn Cell(caption: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-1.5",
            p { class: CAPTION_CLASS, "{caption}" }
            {children}
        }
    }
}

/// The sample board in an update fixture's `row`, with its details open on
/// `open`.
pub(crate) fn update_card(row: UpdateFixtureRow, open: Option<CardPart>) -> Element {
    let fixture = UpdateFixture::new(row, porch());
    rsx! {
        StoryBoardCard {
            card: fixture.view.clone(),
            update: fixture.words(),
            update_facts: fixture.offer_facts(),
            feed: live_feed(),
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            details_open: open,
            on_action: |_| {},
        }
    }
}

/// The porch board after a push that failed: the project bar striped, its
/// Retry.
pub(crate) fn failed_card(open: Option<CardPart>) -> Element {
    let mut card = porch();
    card.last_outcome = Some(OutcomeView {
        summary: "The board did not answer".to_string(),
        ok: false,
    });
    rsx! {
        StoryBoardCard {
            card,
            feed: live_feed(),
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            ended: Some(ActivityEnd {
                kind: DeviceActivityKind::Push,
                ok: false,
                at: STORY_BOARD_NOW - 5.0,
            }),
            details_open: open,
            on_action: |_| {},
        }
    }
}

/// The porch board reached through lightplayer.app.
pub(crate) fn relay_card(open: Option<CardPart>) -> Element {
    rsx! {
        StoryBoardCard {
            card: over_network(porch()),
            feed: live_feed(),
            link: Some(UiLinkKind::Relay),
            lan: lan_link_for_endpoint("relay:a0f26287b48c"),
            // A relay link presents the account's key: it holds edit with it.
            access: Some(UiDeviceAccess {
                grant: Some(lpa_studio_core::UiAccessGrant {
                    tier: lpa_studio_core::AccessTier::Edit,
                    key: Some("your account key".to_string()),
                }),
                ..UiDeviceAccess::default()
            }),
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            details_open: open,
            on_action: |_| {},
        }
    }
}

/// The porch board as another tab's hold leaves it here: its port is there
/// and was never opened (a gated link the model keeps), so nothing is
/// known of what runs on it. `level` is the holder's last word;
/// `taken_from_here` is a tab that let go on request.
pub(crate) fn held_by_another_tab(level: HoldLevel, taken_from_here: bool) -> DeviceView {
    DeviceView {
        status: DeviceStatus::Attached,
        state_label: "Attached — not listening".to_string(),
        freshness_label: None,
        firmware_face: DeviceFirmwareFace::Unknown,
        remembered_firmware: Some("fw-esp32c6 2026.10.05-2".to_string()),
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        can_receive_project: false,
        can_remove_project: false,
        terminal: Vec::new(),
        held_elsewhere: Some(HeldElsewhere {
            via: HoldVia::Usb,
            level,
            taken_from_here,
        }),
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        ..porch()
    }
}

/// The card of a board another tab holds, with `take_over` under way or
/// failed; the picture is the one that tab saved five minutes ago.
pub(crate) fn held_card(
    level: HoldLevel,
    taken_from_here: bool,
    take_over: Option<UiTakeOver>,
) -> Element {
    rsx! {
        StoryBoardCard {
            card: held_by_another_tab(level, taken_from_here),
            feed: saved_by_another_tab(),
            take_over,
            on_action: |_| {},
        }
    }
}

/// The ask is out: the other tab has five seconds to answer.
pub(crate) fn asking_the_other_tab() -> UiTakeOver {
    UiTakeOver {
        words: "Asking the other tab…".to_string(),
        failed: false,
    }
}

/// The picture another tab saved five minutes ago.
pub(crate) fn saved_by_another_tab() -> Option<DeviceCardFeedView> {
    Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(5.0 * 60.0),
        engine_fps: None,
        liveness: FeedLiveness::Offline,
        from_lens: false,
    })
}

/// A connect turned away because someone else holds the board's network
/// connection.
fn busy_connect(through_relay: bool) -> UiWifiConnect {
    UiWifiConnect {
        host: match through_relay {
            true => "lightplayer.app".to_string(),
            false => "192.168.1.40".to_string(),
        },
        through_relay,
        connecting: false,
        error: Some(WIFI_BUSY_WORDS.to_string()),
        busy: true,
    }
}

/// A day, in seconds.
pub(crate) const DAY: f64 = 86_400.0;

/// The porch board's registry uid (its Edit).
pub(crate) const PORCH_UID: &str = "dev000000daqf6dvvqz";

/// What the porch board plays: a project the library cannot name.
pub(crate) fn running() -> BoardPlays {
    BoardPlays::Running {
        label: "aurora-drift".to_string(),
    }
}

/// The spike's sample board: Porch lights, a XIAO ESP32-C6 on its USB
/// cable, running a project at 58 fps.
pub(crate) fn porch() -> DeviceView {
    DeviceView {
        id: DeviceId(7),
        title: "Porch lights".to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
        freshness_label: Some("last heard 1 s ago".to_string()),
        identity_label: Some("60:55:f9:0a:0b:0c".to_string()),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.05-2".to_string()),
            wire: DeviceWireVersion::Match,
            age: DeviceFirmwareAge::Current,
        },
        remembered_firmware: None,
        degraded: None,
        engine_fps: Some(58),
        link_counters: None,
        loaded_project: DeviceLoadedProject::Running {
            label: "aurora-drift".to_string(),
        },
        can_receive_project: true,
        can_remove_project: true,
        activity: None,
        last_outcome: None,
        terminal: porch_terminal(),
        terminal_dropped: 0,
        firmware_blocked: None,
        held_elsewhere: None,
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
    }
}

/// The porch board saying who it is: the connection bar's work.
pub(crate) fn identifying() -> DeviceView {
    DeviceView {
        status: DeviceStatus::Busy,
        state_label: "Identifying".to_string(),
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        can_receive_project: false,
        can_remove_project: false,
        activity: Some(DeviceActivityView {
            kind: DeviceActivityKind::Identify,
            label: "Identifying…".to_string(),
            percent: None,
            cancellable: true,
            cancel_requested: false,
            layout: None,
            update: None,
        }),
        escapes: vec![
            DeviceEscape::Cancel,
            DeviceEscape::Disconnect,
            DeviceEscape::Forget,
        ],
        ..porch()
    }
}

/// The porch board, remembered and not here.
pub(crate) fn offline() -> DeviceView {
    DeviceView {
        status: DeviceStatus::Offline,
        state_label: "Offline".to_string(),
        freshness_label: None,
        firmware_face: DeviceFirmwareFace::Unknown,
        remembered_firmware: Some("fw-esp32c6 2026.10.05-2".to_string()),
        loaded_project: DeviceLoadedProject::Unknown,
        engine_fps: None,
        can_receive_project: false,
        can_remove_project: false,
        terminal: Vec::new(),
        escapes: vec![DeviceEscape::Reconnect, DeviceEscape::Forget],
        ..porch()
    }
}

/// `card` reached over a network link: firmware needs USB there.
pub(crate) fn over_network(card: DeviceView) -> DeviceView {
    DeviceView {
        firmware_blocked: Some(FIRMWARE_NEEDS_USB.to_string()),
        ..card
    }
}

/// An emulated board in this tab, running.
pub(crate) fn emulated() -> DeviceView {
    DeviceView {
        id: DeviceId(9),
        title: "Bench emulator".to_string(),
        identity_label: None,
        ..porch()
    }
}

/// A Bluetooth link nothing has unlocked.
pub(crate) fn locked_access() -> UiDeviceAccess {
    UiDeviceAccess {
        over_bluetooth: true,
        unlock: Some(UiUnlockOffer::Locked),
        ..UiDeviceAccess::default()
    }
}

/// A board just plugged in, settled on needing firmware.
pub(crate) fn new_board() -> PendingLinkView {
    PendingLinkView {
        link: DeviceLinkId(4),
        device: DeviceId(104),
        title: "New board".to_string(),
        state_label: "Blank flash — needs firmware".to_string(),
        detail: None,
        can_adopt: true,
        firmware_face: DeviceFirmwareFace::Blank,
        detected_chip: Some("esp32c6".to_string()),
        mac: Some("60:55:f9:0a:0b:0d".to_string()),
        firmware_blocked: None,
        held_by_tab: false,
        escapes: vec![DeviceEscape::Forget],
    }
}

/// The board's picture, live.
pub(crate) fn live_feed() -> Option<DeviceCardFeedView> {
    Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(0.2),
        engine_fps: Some(58),
        liveness: FeedLiveness::Live,
        from_lens: false,
    })
}

/// The session's own picture: the frames the lens draws while the board is
/// connected (or docked in the editor), live.
pub(crate) fn lens_feed() -> Option<DeviceCardFeedView> {
    Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(0.2),
        engine_fps: Some(57),
        liveness: FeedLiveness::Live,
        from_lens: true,
    })
}

/// The porch board connected on its card, its panel core's picks of
/// `root`. With `access`, it holds that access over Bluetooth (the
/// play-only story); else it is on its USB cable, at the edit tier.
pub(crate) fn connected_card(root: UiPanelGroup, access: Option<UiDeviceAccess>) -> Element {
    let panel = board_panel_picks(&root, Some(true));
    let (card, link) = match access {
        Some(_) => (over_network(porch()), Some(UiLinkKind::Bluetooth)),
        None => (porch(), None),
    };
    rsx! {
        StoryBoardCard {
            card,
            feed: lens_feed(),
            link,
            access,
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            editor_holds_it: true,
            connection: BoardConnection::Connected,
            panel: Some(panel),
            on_action: |_| {},
        }
    }
}

/// Bluetooth unlocked with the play password: play, not edit.
pub(crate) fn play_only_access() -> UiDeviceAccess {
    UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Unlocked with friends · play".to_string()),
        unlock: Some(UiUnlockOffer::PlayOnly),
        grant: Some(lpa_studio_core::UiAccessGrant {
            tier: lpa_studio_core::AccessTier::Play,
            key: Some("friends".to_string()),
        }),
        ..UiDeviceAccess::default()
    }
}

/// The last picture before the board went away, five hours old.
pub(crate) fn last_picture() -> Option<DeviceCardFeedView> {
    Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(5.0 * 3600.0),
        engine_fps: None,
        liveness: FeedLiveness::Offline,
        from_lens: false,
    })
}

/// The porch board with an update waiting to move its files (or refused
/// because they do not fit): core's layout panel over the verbs core
/// publishes for it, raising the firmware details.
fn layout_card(refused: bool) -> Element {
    let base = DeviceView {
        firmware_face: DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: DeviceWireVersion::Match,
            age: DeviceFirmwareAge::Unknown,
        },
        ..porch()
    };
    let waiting = DeviceFlashStep::WaitingForAnswer.label();
    let (card, staged) = match refused {
        false => (
            DeviceView {
                status: DeviceStatus::Busy,
                state_label: waiting.to_string(),
                can_receive_project: false,
                can_remove_project: false,
                activity: Some(DeviceActivityView {
                    kind: DeviceActivityKind::Flash,
                    label: waiting.to_string(),
                    percent: None,
                    cancellable: true,
                    cancel_requested: false,
                    layout: Some(DeviceFlashLayoutView {
                        verdict: DeviceLayoutVerdict::Migrate {
                            files: 9,
                            bytes: 48_128,
                            free_blocks: 150,
                            tight: false,
                            backup_stored: true,
                            device_uid: Some(PORCH_UID.to_string()),
                        },
                        awaiting_consent: true,
                    }),
                    update: None,
                }),
                escapes: vec![
                    DeviceEscape::Cancel,
                    DeviceEscape::Disconnect,
                    DeviceEscape::Forget,
                ],
                ..base
            },
            None,
        ),
        true => (
            DeviceView {
                last_outcome: Some(OutcomeView {
                    summary: "the board's files don't fit the new firmware — nothing was changed"
                        .to_string(),
                    ok: false,
                }),
                ..base
            },
            Some(LayoutStaging {
                verdict: DeviceLayoutVerdict::Refused {
                    files: 40,
                    blocks_needed: Some(170),
                    blocks_total: 176,
                    blocks_reserved: 16,
                    block_bytes: 4096,
                },
                plan: None,
                archive: None,
                restoring: None,
                downloaded: false,
            }),
        ),
    };
    let mut verbs = UiOfferTree::new();
    let layout = device_layout_view(
        &card,
        story_board_prefix(card.id),
        DeviceBoardFs::Mounted,
        true,
        staged.as_ref(),
        None,
        None,
        &mut verbs,
    );
    rsx! {
        section { class: DETAILS_FRAME_CLASS,
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card,
                    feed: live_feed(),
                    open_uid: Some(PORCH_UID.to_string()),
                    plays: running(),
                    layout,
                    extra_offers: Some(verbs),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// The board's Wi‑Fi, as it reports it: on, Cloud relay on, `networks`
/// saved, its station `station`.
fn wifi_board(station: StationState, networks: Vec<SavedNetworkInfo>) -> UiDeviceWifi {
    UiDeviceWifi {
        status: Some(NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks,
            station,
            relay: RelayState::Off,
        }),
        ..UiDeviceWifi::new(DeviceId(7), true)
    }
}

/// On the truck's network, the apartment's password refused last time,
/// the house out of range.
fn connected_truck() -> UiDeviceWifi {
    let mut wifi = wifi_board(
        StationState::Connected {
            ssid: "Starlink Truck".to_string(),
            ip: "192.168.1.17".to_string(),
            rssi: -41,
            host: "lp-8e30.local".to_string(),
        },
        vec![
            saved_network("Starlink Home", Some(LastAttempt::NotFound)),
            saved_network("Starlink Truck", Some(LastAttempt::Connected)),
            saved_network("Starlink Apt", Some(LastAttempt::WrongPassword)),
        ],
    );
    wifi.heard = Some(vec![
        heard_network("Starlink Truck", -41, true),
        heard_network("Pixel_7310", -66, true),
    ]);
    wifi
}

fn saved_network(ssid: &str, last: Option<LastAttempt>) -> SavedNetworkInfo {
    SavedNetworkInfo {
        ssid: ssid.to_string(),
        has_password: true,
        hidden: false,
        last,
    }
}

fn heard_network(ssid: &str, rssi: i8, secure: bool) -> HeardNetwork {
    HeardNetwork {
        ssid: ssid.to_string(),
        rssi,
        secure,
    }
}

/// One terminal line, said once.
fn terminal_line(kind: DeviceTerminalKind, text: &str) -> DeviceTerminalLine {
    DeviceTerminalLine {
        kind,
        text: text.to_string(),
        repeats: 1,
    }
}

/// What a running board says, in order: its boot, its hello, the
/// heartbeat collapsed.
fn porch_terminal() -> Vec<DeviceTerminalLine> {
    let line = |kind, text: &str, repeats| DeviceTerminalLine {
        kind,
        text: text.to_string(),
        repeats,
    };
    vec![
        line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919", 1),
        line(
            DeviceTerminalKind::Board,
            "[INIT] fw-esp32c6 initialized, starting server loop",
            1,
        ),
        line(
            DeviceTerminalKind::Board,
            "[INIT] loaded /projects/aurora-drift",
            1,
        ),
        line(
            DeviceTerminalKind::Wire,
            "hello · seeed/xiao-esp32-c6 · fw-esp32c6 2026.10.05-2",
            1,
        ),
        line(DeviceTerminalKind::Studio, "Opened the port", 1),
        line(DeviceTerminalKind::Outcome, "Identified in 0.4 s", 1),
        line(
            DeviceTerminalKind::Wire,
            "heartbeat · 58 fps · heap 108 KB · aurora-drift",
            6,
        ),
    ]
}

/// The states row: as many 244 px cards as fit (the spike's grid).
const STORY_GRID_CLASS: &str =
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(244px,1fr))] tw:items-start tw:gap-3.5 tw:p-4";

/// A details story's frame: room under the card for the open card.
const DETAILS_FRAME_CLASS: &str = "tw:min-h-[900px] tw:p-4";

/// One card at a desktop card's width.
const CARD_WIDTH_CLASS: &str = "tw:w-[300px] tw:max-w-full";

/// A cell's caption.
const CAPTION_CLASS: &str =
    "tw:m-0 tw:text-[10.5px] tw:font-bold tw:uppercase tw:tracking-[0.06em] tw:text-dim-foreground";
