//! The board card's stories: every state in one row (the card's one height
//! is measured here, AC5), each bar's details open, the corner's, a tall
//! details card over its own bar, and the narrow card.
//!
//! Every card is built by core's own builder ([`StoryBoardCard`] calls
//! `board_card` over the tree core would publish), so a story card says and
//! offers exactly what core decides for its board.

use dioxus::prelude::*;
use lpa_studio_core::{
    ActivityEnd, BarLayer, BoardPlays, DeviceActivityKind, DeviceActivityView, DeviceCardFeedView,
    DeviceEscape, DeviceFirmwareAge, DeviceFirmwareFace, DeviceId, DeviceLinkId,
    DeviceLoadedProject, DeviceStatus, DeviceTerminalKind, DeviceTerminalLine, DeviceView,
    DeviceWireVersion, FIRMWARE_NEEDS_USB, FeedLiveness, OutcomeView, PendingLinkView,
    UiDeviceAccess, UiLinkKind, UiRuntimeBand, UiUnlockOffer, UpdateFixture, UpdateFixtureRow,
    lan_link_for_endpoint,
};
use lpa_studio_web_story_macros::story;

use super::CardPart;
use crate::app::home::device_offer_story_fixtures::{
    STORY_BOARD_NOW, StoryBoardCard, StoryNewBoardCard,
};
use crate::app::home::home_gallery_stories::live_card_lamp_frame;

// --- Every state ------------------------------------------------------------

#[story(
    description = "The board card in every state, one height each (AC5): live (\"58 fps\" beside the blue dot, Edit as the primary), connecting (the connection bar working: spinner, \"Identifying…\", the iridescent sweep along its foot, Cancel), offline with its last picture (dimmed, \"2 weeks\" in the corner, Connect over Wi‑Fi), locked (over Bluetooth: Unlock, the picture dark), updating (the firmware bar's work at 40 % and the board's own light strip in the picture), done (the project bar green for a few seconds), failed (striped, Retry), a new board (Install with the board pick), an emulated board (\"Emulated XIAO ESP32-C6\", in this tab) and a board reached through lightplayer.app (\"Wi‑Fi via lightplayer.app · live\", the cloud icon). The picture is 138 px, the name bar 50 px, each bar 28 px: every card is 330 px tall."
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
        }
    }
}

// --- Each part's details, open ---------------------------------------------

#[story(
    description = "The project bar's details, open: Studio's detail card merged with the bar that opened it (the same shared popover, its trigger the bar's whole width). The project the board plays, then its verbs as menu rows — Put another project on it (the project pick), Clear faults where offered — and Remove project apart, behind the danger zone's red line, arming in place on two clicks."
)]
fn board_card_project_details_open() -> Element {
    details_open(BarLayer::Project)
}

#[story(
    description = "The connection bar's details, open: how the board is reached (the link, the address, the last word heard), then Identify again and Disconnect as menu rows."
)]
fn board_card_connection_details_open() -> Element {
    details_open(BarLayer::Connection)
}

#[story(description = "The access bar's details, open: who can do what on this link.")]
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
    description = "The hardware bar's details, open: the board, its chip and its id (the MAC, shown once), Reset, and Forget apart behind the danger zone's line."
)]
fn board_card_hardware_details_open() -> Element {
    details_open(BarLayer::Hardware)
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
    description = "The 2026-10-06 regression case: a details card taller than the room below its bar, opened from the card's last bar near the bottom of a short viewport. The shared popover clamps the card into view across its own wide bar, and the bar's copy is not drawn over the card's rows (`docs/defects/2026-10-06-a-card-row-shows-through-its-own-popover.md`)."
)]
fn board_card_tall_details_over_its_bar() -> Element {
    rsx! {
        section { class: "tw:flex tw:min-h-[760px] tw:items-end tw:p-4",
            div { class: CARD_WIDTH_CLASS,
                StoryBoardCard {
                    card: porch(),
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
            open_uid: Some(PORCH_UID.to_string()),
            plays: running(),
            details_open: open,
            on_action: |_| {},
        }
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
    })
}

/// The last picture before the board went away, five hours old.
pub(crate) fn last_picture() -> Option<DeviceCardFeedView> {
    Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(5.0 * 3600.0),
        engine_fps: None,
        liveness: FeedLiveness::Offline,
    })
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
const DETAILS_FRAME_CLASS: &str = "tw:min-h-[760px] tw:p-4";

/// One card at a desktop card's width.
const CARD_WIDTH_CLASS: &str = "tw:w-[300px] tw:max-w-full";

/// A cell's caption.
const CAPTION_CLASS: &str =
    "tw:m-0 tw:text-[10.5px] tw:font-bold tw:uppercase tw:tracking-[0.06em] tw:text-dim-foreground";
