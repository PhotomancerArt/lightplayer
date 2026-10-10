//! Firmware-update stories (the update-states spike, direction C): the
//! device card in every row of the update table, the editor's device
//! popover in its four words, and the editor's curtain while the board
//! updates.
//!
//! Every board here is built from core's own update fixtures
//! ([`UpdateFixture`]): a board manifest as the board reports it, this
//! Studio's build facts, the card with its Update activity — read through
//! the same standing, words and offers the controller uses. So a card says
//! and offers exactly what core decides for that row, and a story cannot
//! drift from it. The sample board is the spike's: on `2026.10.03-1`, with
//! this Studio carrying `2026.10.05-2`.
//!
//! What to look at, row by row: the firmware line (core's words, the
//! board's version set in mono) and its bar (lit while an update runs —
//! quieter when another device runs it); the header chip (the update's
//! word while it owns the board) and the second identity row (the version
//! leading, its commit dim); the verb row (only what core offers); and,
//! where the show has stopped, the picture slot showing the board's own
//! light with the whole sentence.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceCardFeedView, DeviceEscape, DeviceFace, DeviceId, DeviceLoadedProject, DeviceStatus,
    DeviceView, FIRMWARE_NEEDS_USB, FeedLiveness, INSTALL_FIND_PARAM, INSTALL_VERSION_PARAM,
    OfferArgs, UiChromeSessionControl, UiChromeSessionStatus, UiDeviceAccess, UiLensReconnecting,
    UiUnlockOffer, UpdateFixture, UpdateFixtureRow, lan_link_for_endpoint, looked_up_release,
};
use lpa_studio_web_story_macros::story;

use crate::app::board_card::other_version_form::OfferPickerPreview;
use crate::app::home::device_offer_story_fixtures::{StoryDeviceCard, session_device_tree};
use crate::app::home::home_gallery_stories::live_card_lamp_frame;
use crate::app::layout::LinkReconnectingStrip;
use crate::app::layout::session_control::SessionDevicePanel;
use crate::core::OffersProvider;

// --- The device card, one row of the table each ---------------------------

#[story(
    description = "Up to date (information): the firmware bar says the version alone, \"2026.10.05-2\", plain and with no action; its details give the version with its commit and build, Other version… (the store's other releases), and Factory reset apart."
)]
fn device_card_update_up_to_date() -> Element {
    update_card(UpdateFixtureRow::UpToDate, Link::Usb)
}

#[story(
    description = "Update available (information with an offer, not a needs-you): the firmware bar is blue with \"2026.10.03-1\" and one plain Update (Routine: one click, no arm); Other version… and Factory reset are in its details. The show keeps running."
)]
fn device_card_update_available() -> Element {
    update_card(UpdateFixtureRow::Available, Link::Usb)
}

#[story(
    description = "Update available on a dev-build board: the firmware bar is blue with \"dev 5eb70a7\", its action \"Install 2026.10.05-2\" instead of Update, because a dev build has no order against a release; its details name both builds."
)]
fn device_card_update_available_dev_board() -> Element {
    update_card(UpdateFixtureRow::AvailableDevBoard, Link::Usb)
}

#[story(
    description = "Backing up (progress): the current firmware is read back before a byte is written. The firmware bar's work reads \"Backing up · 18%\", its foot lit to 18%, with Cancel — nothing on the board has changed yet, and the picture keeps the show."
)]
fn device_card_update_backing_up() -> Element {
    update_card(UpdateFixtureRow::BackingUp, Link::Usb)
}

#[story(
    description = "Updating over USB (progress, the show stopped): \"Updating · 1 of 2 · 40%\" on the firmware bar, no Cancel once writing starts, and the picture shows the board's own lights — solid dark yellow. The whole sentence, naming the link, is in its details and the corner's: \"Updating to 2026.10.05-2 over USB: the new firmware first. Keep the board powered.\""
)]
fn device_card_update_updating_usb() -> Element {
    update_card(UpdateFixtureRow::Updating, Link::Usb)
}

#[story(
    description = "Updating over Bluetooth: same work (\"Updating · 1 of 2 · 40%\"), its sentence naming the link (\"…over Bluetooth: the new firmware first.\")."
)]
fn device_card_update_updating_bluetooth() -> Element {
    update_card(UpdateFixtureRow::Updating, Link::Bluetooth)
}

#[story(
    description = "Updating over Wi‑Fi (the board on the LAN, no cable): the same work (\"Updating · 1 of 2 · 40%\"), its sentence naming the link (\"…over Wi‑Fi: the new firmware first.\"), the connection bar \"Wi‑Fi · live\". The board resets three times on the way; each time the page redials it by itself and the card keeps this work."
)]
fn device_card_update_updating_wifi() -> Element {
    update_card(UpdateFixtureRow::Updating, Link::Wifi)
}

#[story(
    description = "The last step of an update this Studio is running (progress): \"Updating · 2 of 2 · 70%\", the strip dark yellow, and \"Installing the rest of the firmware. Keep the board powered.\" Not called interrupted: nothing was."
)]
fn device_card_update_finishing() -> Element {
    update_card(UpdateFixtureRow::Finishing, Link::Bluetooth)
}

#[story(
    description = "Finishing an interrupted update this Studio found half-way on connect (progress, started with no click): \"Resuming · 2 of 2 · 70%\", naming the piece it is on, the strip dark yellow, and the sentence saying it was interrupted and this Studio is installing the rest."
)]
fn device_card_update_finishing_resumed() -> Element {
    update_card(UpdateFixtureRow::FinishingResumed, Link::Bluetooth)
}

#[story(
    description = "Restoring missing firmware (progress, no click): \"Restoring · 35%\" on the firmware bar, the strip dark yellow: part of it was missing and Studio had a copy."
)]
fn device_card_update_restoring() -> Element {
    update_card(UpdateFixtureRow::Restoring, Link::Usb)
}

#[story(
    description = "Another device is updating it (progress, someone else's): \"Another device is updating it · 40%\", the foot at 40% in the quieter fill, no Cancel, the strip dark yellow: this Studio finishes it if it stops."
)]
fn device_card_update_another_device() -> Element {
    update_card(UpdateFixtureRow::AnotherDevice, Link::Bluetooth)
}

#[story(
    description = "Needs one update over USB, plugged in by USB (needs you): the firmware bar's words in orange, and today's USB flash as its action (Update, Lasting: it arms on the first click)."
)]
fn device_card_update_needs_usb_once_usb() -> Element {
    update_card(UpdateFixtureRow::NeedsUsbOnce, Link::Usb)
}

#[story(
    description = "Needs one update over USB, reached over Bluetooth: the same orange words, and nothing to press here — the details say what to do (update it over USB once, and after that it updates without a cable)."
)]
fn device_card_update_needs_usb_once_bluetooth() -> Element {
    update_card(UpdateFixtureRow::NeedsUsbOnce, Link::Bluetooth)
}

#[story(
    description = "Over Wi‑Fi, a board released before Wi‑Fi updates: its hello offered the update, but its Wi‑Fi link never answered it (5 s), so the firmware bar says what to do — \"Update over USB or Bluetooth once\"; details: \"This board updates over USB or Bluetooth until it has been updated once.\" — and offers nothing that would hang."
)]
fn device_card_update_not_over_wifi_yet() -> Element {
    update_card(UpdateFixtureRow::NotOverWifiYet, Link::Wifi)
}

#[story(
    description = "Keeps crashing (needs you, the show stopped): \"2026.10.03-1 keeps crashing\" in orange with Reinstall, the picture's strip dark red, and in the details the whole sentence and Other version… (the store's releases, the board's own drawn but not pickable). Factory reset is withdrawn: installing is the repair."
)]
fn device_card_update_keeps_crashing() -> Element {
    update_card(UpdateFixtureRow::KeepsCrashing, Link::Usb)
}

#[story(
    description = "Needs a version this Studio can't get (needs you, the show stopped): \"Needs 2026.09.28-4, which Studio can't get\" in orange, the strip dark red, the details saying to connect to the internet or install this Studio's version, with the install there, 2026.10.05-2 preselected. Factory reset withdrawn."
)]
fn device_card_update_cant_get_version() -> Element {
    update_card(UpdateFixtureRow::CantGetVersion, Link::Usb)
}

#[story(
    description = "Rolled back (information): the update didn't start, so the board is back on 2026.10.03-1 and refuses that build: no Update, only the version on the firmware bar, and Other version… shows the refused build unpickable. The show runs on the old one."
)]
fn device_card_update_rolled_back() -> Element {
    update_card(UpdateFixtureRow::RolledBack, Link::Usb)
}

#[story(
    description = "Newer than this Studio (information): the firmware bar says \"2026.10.07-4\", plain; its details also name this Studio's own, and offer Other version… (an older one arms first)."
)]
fn device_card_update_newer() -> Element {
    update_card(UpdateFixtureRow::Newer, Link::Usb)
}

#[story(
    description = "Play access only, over Bluetooth: the update is available and said so — the firmware bar is blue — but its Update wears a lock: it needs the author password, so it opens the Unlock sheet."
)]
fn device_card_update_play_only() -> Element {
    update_card(UpdateFixtureRow::PlayOnly, Link::Bluetooth)
}

// --- "Other version…": the picker over the store's release index ----------

#[story(
    description = "Other version… open on an up-to-date board: a box to find or type a version above the store's newest five releases, newest first (version in mono, its publish time), the board's own drawn but not pickable (\"On this board now\"). The newest is picked, and its press reads Install: one click (Routine: newer than the board's). From a file… beside the press picks a custom build's files."
)]
fn device_card_update_other_version_picker() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb)),
        OfferArgs::new(),
        false,
    )
}

#[story(
    description = "Typing in the box filters the whole list, not just the five: \"10.03\" finds the four releases of Oct 3, newest first, each with its older-language warning. Nothing is picked until one is clicked, so the press says to pick one."
)]
fn device_card_update_other_version_picker_filtered() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb)),
        OfferArgs::new().with(INSTALL_FIND_PARAM, "10.03"),
        false,
    )
}

#[story(
    description = "An older version typed whole and armed (Lasting): the box names 2026.10.05-1, the list shows it picked, and the panel says what changes in core's words — \"Install an older version?\" and that it may not read the board's project — above the armed \"Confirm install\"."
)]
fn device_card_update_other_version_picker_older_armed() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb)),
        OfferArgs::new().with(INSTALL_FIND_PARAM, "2026.10.05-1"),
        true,
    )
}

#[story(
    description = "An older version that also speaks an older wire language, armed: the row's warning (\"older language than Studio\"), and both sentences in the copy — older version, and Studio can still update it but may not edit its project until you do."
)]
fn device_card_update_other_version_picker_wire_warning() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb)),
        OfferArgs::new()
            .with(INSTALL_FIND_PARAM, "10.03")
            .with(INSTALL_VERSION_PARAM, "2026.10.03-4"),
        true,
    )
}

#[story(
    description = "A whole version older than the store's list holds: nothing in the list matches, so the press reads \"Look up 2026.09.30-2\" — one click (Routine) asks the store for that release by its version."
)]
fn device_card_update_other_version_picker_lookup() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb)),
        OfferArgs::new().with(INSTALL_FIND_PARAM, "2026.09.30-2"),
        false,
    )
}

#[story(
    description = "The same version once the store has found it: it joins the list (picked, with its older-language warning, dated by its version — a lookup carries no publish time), and the press installs it — armed here, with both sentences, since it is older than the board's."
)]
fn device_card_update_other_version_picker_lookup_found() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb))
            .looked_up("2026.09.30-2", looked_up_release("2026.09.30-2")),
        OfferArgs::new().with(INSTALL_FIND_PARAM, "2026.09.30-2"),
        true,
    )
}

#[story(
    description = "After \"From a file…\": a custom build's update files picked from this computer (its ota-manifest.json, core.bin and engine.bin), checked by core against their manifest. The build leads the list, \"from your files\", picked; its install always arms, and the copy says it is a custom build that no store vouches for."
)]
fn device_card_update_other_version_picker_from_file() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::UpToDate, porch_lights(Link::Usb))
            .with_file_build("9c1e4b7a2"),
        OfferArgs::new(),
        true,
    )
}

#[story(
    description = "Other version… with the store's list unreachable, on a board newer than this Studio: only this Studio's build to pick, one quiet line saying the full list isn't available right now, and the older-version copy above the press."
)]
fn device_card_update_other_version_picker_offline() -> Element {
    update_picker(
        UpdateFixture::new(UpdateFixtureRow::Newer, porch_lights(Link::Usb)).offline(),
        OfferArgs::new(),
        false,
    )
}

// --- The editor's device popover ------------------------------------------

#[story(
    description = "The editor's device popover on a board that is up to date: the popover's own run word (\"running\") stands, and the stat line states the version: \"esp32c6 · 2026.10.05-2 · <mac>\"."
)]
fn device_popover_update_running() -> Element {
    update_popover(UpdateFixtureRow::UpToDate, UiChromeSessionStatus::Run)
}

#[story(
    description = "The device popover with an update on offer: \"running · update available\" in the live tone, and \"esp32c6 · 2026.10.03-1 → 2026.10.05-2 · <mac>\"."
)]
fn device_popover_update_available() -> Element {
    update_popover(UpdateFixtureRow::Available, UiChromeSessionStatus::Run)
}

#[story(
    description = "The device popover while the board updates: \"Updating · 40%\" in the working tone."
)]
fn device_popover_update_updating() -> Element {
    update_popover(UpdateFixtureRow::Updating, UiChromeSessionStatus::Run)
}

#[story(
    description = "The device popover on a board whose firmware keeps crashing: \"Needs firmware\" in the attention tone, and \"esp32c6 · 2026.10.03-1 keeps crashing · <mac>\"."
)]
fn device_popover_update_needs_firmware() -> Element {
    update_popover(
        UpdateFixtureRow::KeepsCrashing,
        UiChromeSessionStatus::Attention,
    )
}

// --- The editor's curtain -------------------------------------------------

#[story(
    description = "The editor's Reconnecting card while its board updates over Bluetooth: the board's link drops and comes back as it resets into the new firmware, and that is the update, not a lost connection — so the card's detail is the update's own line, core's words."
)]
fn device_curtain_update_updating() -> Element {
    let fixture = UpdateFixture::new(UpdateFixtureRow::Updating, porch_lights(Link::Bluetooth));
    let line = fixture.words().map(|words| words.line).unwrap_or_default();
    rsx! {
        section { class: "tw:grid tw:w-[760px] tw:gap-3 tw:p-4",
            LinkReconnectingStrip { reconnecting: UiLensReconnecting::updating(&line) }
        }
    }
}

#[story(
    description = "The same Reconnecting card while its board updates over Wi‑Fi: each reset closes the board's socket and the page redials it by itself, so the card's detail is the update's line, \"Updating · 1 of 2 · 40%\"."
)]
fn device_curtain_update_updating_wifi() -> Element {
    let fixture =
        UpdateFixture::new(UpdateFixtureRow::Updating, porch_lights(Link::Wifi)).over_wifi();
    let line = fixture.words().map(|words| words.line).unwrap_or_default();
    rsx! {
        section { class: "tw:grid tw:w-[760px] tw:gap-3 tw:p-4",
            LinkReconnectingStrip { reconnecting: UiLensReconnecting::updating(&line) }
        }
    }
}

// --- Helpers --------------------------------------------------------------

/// The frame the device-card stories use.
const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";

/// The link a story's board is reached over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Link {
    Usb,
    Bluetooth,
    /// The board on the LAN (`lan:ws://192.168.1.40/link`).
    Wifi,
}

/// The LAN board's endpoint.
const WIFI_ENDPOINT: &str = "lan:ws://192.168.1.40/link";

/// The sample board in `row`, over `link`: its words and its offers both
/// read by core from the fixture's facts.
fn update_card(row: UpdateFixtureRow, link: Link) -> Element {
    let fixture = UpdateFixture::new(row, porch_lights(link));
    let fixture = match link {
        Link::Wifi => fixture.over_wifi(),
        Link::Usb | Link::Bluetooth => fixture,
    };
    fixture_card(fixture, row, link, None)
}

/// The sample board over USB with its install verb's list open, `args`
/// picked, and armed when `armed`.
fn update_picker(fixture: UpdateFixture, args: OfferArgs, armed: bool) -> Element {
    let row = UpdateFixtureRow::UpToDate;
    // Room under the card for the open list, so a capture holds all of it.
    rsx! {
        div { class: "tw:min-h-[1040px]",
            {fixture_card(fixture, row, Link::Usb, Some(OfferPickerPreview { args, armed }))}
        }
    }
}

/// `fixture`'s card (`row` decides only the play row's access line).
fn fixture_card(
    fixture: UpdateFixture,
    row: UpdateFixtureRow,
    link: Link,
    install_picker_preview: Option<OfferPickerPreview>,
) -> Element {
    let update = fixture.words();
    let update_facts = fixture.offer_facts();
    // Every board streams its picture to the card: over Bluetooth at a
    // gentler pace, which its pill says.
    let feed = Some(DeviceCardFeedView {
        frame: Some(live_card_lamp_frame()),
        frame_age_secs: Some(0.2),
        engine_fps: Some(43),
        liveness: FeedLiveness::Live,
    });
    // Only the play row tells its access: unlocked with friends, for play.
    let access = (row == UpdateFixtureRow::PlayOnly).then(|| UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Unlocked with friends · play".to_string()),
        unlock: Some(UiUnlockOffer::PlayOnly),
        panel: None,
        account_key_refused: None,
        grant: Some(lpa_studio_core::UiAccessGrant {
            tier: lpa_studio_core::AccessTier::Play,
            key: Some("friends".to_string()),
        }),
        waiting: None,
    });
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: fixture.view.clone(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                feed,
                access,
                update,
                update_facts,
                install_picker_preview,
                lan: (link == Link::Wifi).then(|| lan_link_for_endpoint(WIFI_ENDPOINT)).flatten(),
                on_action: |_| {},
            }
        }
    }
}

/// The editor's device popover on the sample board in `row`, over USB.
fn update_popover(row: UpdateFixtureRow, status: UiChromeSessionStatus) -> Element {
    let fixture = UpdateFixture::new(row, porch_lights(Link::Usb));
    let session = UiChromeSessionControl {
        face: DeviceFace::Wire,
        key: "device:dev000000daqf6dvvqz".to_string(),
        device: Some(fixture.view.id),
        name: fixture.view.title.clone(),
        board: None,
        status,
        stat_line: Some("43 fps".to_string()),
        update: fixture.session_words(),
        link: lpa_studio_core::UiLinkKind::Usb,
        connected: false,
    };
    let offers = session_device_tree(session.device, &session.name);
    rsx! {
        div { class: "tw:p-4",
            div { class: POPOVER_FRAME,
                OffersProvider { offers,
                    SessionDevicePanel { session }
                }
            }
        }
    }
}

/// The popover's panel box (the header popover primitive owns it in the
/// app; a story mounting the panel supplies one).
const POPOVER_FRAME: &str = "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:min-w-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-sm tw:text-muted-foreground tw:shadow-lg";

/// The spike's sample board: Porch lights, a XIAO ESP32-C6 running a
/// project, on a USB cable or reached over Bluetooth.
fn porch_lights(link: Link) -> DeviceView {
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
        firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 a41c9e2".to_string()),
            wire: lpa_studio_core::DeviceWireVersion::Match,
            age: lpa_studio_core::DeviceFirmwareAge::Unknown,
        },
        remembered_firmware: None,
        degraded: None,
        engine_fps: Some(43),
        link_counters: None,
        loaded_project: DeviceLoadedProject::Running {
            label: "aurora-drift".to_string(),
        },
        can_receive_project: true,
        can_remove_project: true,
        activity: None,
        last_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: (link != Link::Usb).then(|| FIRMWARE_NEEDS_USB.to_string()),
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
        update_blocked: None,
        last_update_outcome: None,
    }
}
