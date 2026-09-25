//! Bluetooth access stories (BLE M6, G3): the password sheet, the device
//! access panel's states, the add slot's per-browser Bluetooth notes, a card
//! reached over Bluetooth (flash disabled with its reason, the login line),
//! the account default setting, and the project's Bluetooth list.
//!
//! Every one of these is also captured at the phone width (the story
//! harness's `sm` viewport), which is where G3 reviews them.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessTier, DeviceEscape, DeviceId, DeviceLinkId, DeviceLoadedProject, DeviceStatus,
    DeviceView, FIRMWARE_NEEDS_USB, PendingLinkView, UiAccessPanel, UiAccessSecret, UiDeviceAccess,
    UiDeviceSettingsView, UiLoginPrompt, UiProjectAccess,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_reach::BleReach;
use crate::app::home::bluetooth_settings_section::BluetoothSettingsSection;
use crate::app::home::device_access_panel::DeviceAccessPanel;
use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
use crate::app::home::devices_page::AddDeviceCard;
use crate::app::home::login_sheet::LoginSheet;
use crate::app::project::project_bluetooth_section::ProjectBluetoothSection;

#[story(
    description = "The password sheet (BLE M6), three reasons. Top: a piece asked and Studio knew nothing to try (no default, none remembered). Middle: the passwords tried were refused, and the piece's backoff is said as a time, never as \"failed\". Bottom: an edit was refused on a play login. Remember on this browser is on by default; Not now closes it and the card keeps a Log in verb. On a phone it rises from the bottom; on a wide window it is a centred card."
)]
fn ble_login_sheet() -> Element {
    let prompt = |reason: &str, retry: Option<u64>| UiLoginPrompt {
        device: DeviceId(7),
        device_name: "PLAYFUL choker".to_string(),
        reason: reason.to_string(),
        retry_after_ms: retry,
        busy: false,
    };
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:p-3",
            LoginSheet {
                prompt: prompt("PLAYFUL choker asks for a password.", None),
                on_access: |_| {},
                inline: true,
                typed: Some("s'mores".to_string()),
            }
            LoginSheet {
                prompt: prompt("That password didn't open PLAYFUL choker. It will listen again in 4 s.", Some(3_500)),
                on_access: |_| {},
                inline: true,
            }
            LoginSheet {
                prompt: prompt("This needs an edit password. You're logged in to PLAYFUL choker to play only.", None),
                on_access: |_| {},
                inline: true,
            }
        }
    }
}

#[story(
    description = "The device access panel, OFF (or never set from this browser): Turn on, locked — the account default password pre-filled (shown here; it is hidden by default), named \"default\" at edit — or, as an explicit second choice, open with no password, play only."
)]
fn ble_access_panel_off() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(None, false, Vec::new(), false),
                on_access: |_| {},
                show_passwords: true,
            }
        }
    }
}

#[story(
    description = "The device access panel, ON and LOCKED, just after it was turned on over USB: the board reads the Bluetooth switch once at boot, so the amber note says it turns on at the next restart and offers Restart now. The passwords are the ones THIS browser wrote, by name and what they can do; Add sits under the list, where the new row appears; the panel says plainly the piece may hold others from another browser, and Replace all is the way back."
)]
fn ble_access_panel_locked() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(
                    Some(true),
                    false,
                    vec![secret("default", AccessTier::Edit), secret("camp", AccessTier::Play)],
                    true,
                ),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "The device access panel, ON and OPEN: anyone nearby can play with no password, and editing still needs one — said first, because it is the thing to know about an open piece."
)]
fn ble_access_panel_open() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(Some(true), true, vec![secret("default", AccessTier::Edit)], false),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "The add slot, per browser (BLE M5 copy, G3 rework). \"Connect a board\", then \"via USB\" and \"via Bluetooth\" — BOTH always drawn; one this browser cannot drive is DISABLED with its reason under it and a way to continue. Chrome/Edge: both live. Brave: Bluetooth disabled, the flag's address as select-and-copy text (a page cannot open brave://). Firefox and desktop Safari: both disabled, both need Chrome or Edge, and this page's address is given ONCE to open there. iPhone Safari (and Chrome on iOS): USB needs a computer; Bluetooth needs Bluefy — a link to it on the App Store, then this page's address to open in it. Bluefy: USB disabled with the address to open on a computer, Bluetooth live and solid. Bluetooth off: turn it on and reload. Never a generic \"connect failed\"."
)]
fn ble_add_slot_by_browser() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3 tw:sm:grid-cols-2",
            for (browser , reach , usb) in ADD_SLOT_BROWSERS {
                div { key: "{browser}", class: "tw:grid tw:content-start tw:gap-1.5",
                    p { class: "tw:m-0 tw:text-[11px] tw:font-semibold tw:tracking-wide tw:text-dim-foreground tw:uppercase",
                        "{browser}"
                    }
                    AddSlotAs { reach, usb }
                }
            }
        }
    }
}

#[story(
    description = "The add slot in Chrome or Edge on a computer (G3): \"Connect a board\", both buttons live — via USB the spectrum Primary, via Bluetooth the outline beside it — and \"start a board here\" below."
)]
fn ble_add_slot_chrome() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Ready, usb: true } }
}

#[story(
    description = "The add slot in Brave (G3): via USB live; via Bluetooth DISABLED — \"Brave keeps Bluetooth behind a flag.\" — with the flag's address as select-and-copy text, because a page cannot open a brave:// link."
)]
fn ble_add_slot_brave() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Brave, usb: true } }
}

#[story(
    description = "The add slot in Firefox (G3): both buttons DISABLED — USB needs Chrome or Edge on a computer, Bluetooth needs Chrome or Edge — and this page's address, once, as select-and-copy text to open there."
)]
fn ble_add_slot_firefox() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Firefox, usb: false } }
}

#[story(
    description = "The add slot in Safari on iPhone — and Chrome on iPhone, which is the same WebKit (G3): via USB DISABLED (it needs a computer, with this page's address to open there); via Bluetooth DISABLED with the way through: \"Get Bluefy on the App Store\", then this page's address to open in Bluefy."
)]
fn ble_add_slot_iphone_safari() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Ios, usb: false } }
}

#[story(
    description = "The add slot in Bluefy on iPhone (G3): Web Bluetooth but no Web Serial. via USB DISABLED with its reason and this page's address to open on a computer; via Bluetooth live, and the slot's solid verb."
)]
fn ble_add_slot_bluefy() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Ready, usb: false } }
}

/// Each browser as the real ones pair Bluetooth reach with Web Serial.
const ADD_SLOT_BROWSERS: [(&str, BleReach, bool); 7] = [
    ("Chrome / Edge", BleReach::Ready, true),
    ("Brave", BleReach::Brave, true),
    ("Firefox", BleReach::Firefox, false),
    ("Safari (Mac)", BleReach::Safari, false),
    ("iPhone Safari / Chrome", BleReach::Ios, false),
    ("Bluefy (iPhone)", BleReach::Ready, false),
    ("Chrome, Bluetooth off", BleReach::Off, true),
];

/// The add slot pinned to one browser's answers, with the product's own
/// address in its copy lines (never the story server's).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddSlotAs(reach: BleReach, usb: bool) -> Element {
    rsx! {
        div { class: "tw:p-3",
            AddDeviceCard {
                ble_reach: Some(reach),
                usb_available: usb,
                page_url: Some("https://lightplayer.app/devices".to_string()),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A piece reached over Bluetooth (BLE M5/M6). The device line leads with the login — \"Logged in as camp — play\" — and the Firmware zone's verbs are drawn DISABLED with the reason under them, \"Firmware updates need USB\"; Reset likewise needs USB. At play the Device zone offers Log in for edit. Right: the same piece logged in at edit, where the Bluetooth panel's trigger replaces it."
)]
fn ble_device_card_over_bluetooth() -> Element {
    let play = UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Logged in as camp — play".to_string()),
        log_in: Some("Log in for edit".to_string()),
        panel: None,
    };
    let edit = UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Logged in as default — edit".to_string()),
        log_in: None,
        panel: Some(panel(
            Some(true),
            false,
            vec![secret("default", AccessTier::Edit)],
            false,
        )),
    };
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3 tw:sm:grid-cols-2",
            DeviceRosterCard {
                card: ble_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(play),
                on_action: |_| {},
            }
            DeviceRosterCard {
                card: ble_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(edit),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A Bluetooth link still identifying (BLE M6 fix): the pending card's Reset is drawn DISABLED with its reason, \"Reset needs USB\" — a Bluetooth link has no reset lines in any card state, not only once it has settled. Right: a USB link at the same stage, whose Reset stays live (it is the recovery for a silent chip). Below: the Bluetooth link once its check settled on needs-firmware — Flash is drawn DISABLED with \"Firmware updates need USB\", never the live board pick."
)]
fn ble_pending_card_over_bluetooth() -> Element {
    let usb = PendingLinkView {
        link: DeviceLinkId(8),
        device: DeviceId(108),
        title: "Fake ESP32 (usb-8)".to_string(),
        state_label: "New device found — identifying…".to_string(),
        detail: Some("found 2 s ago".to_string()),
        can_adopt: true,
        firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
        detected_chip: None,
        mac: None,
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Forget],
    };
    let ble = PendingLinkView {
        link: DeviceLinkId(9),
        device: DeviceId(109),
        title: "PLAYFUL choker".to_string(),
        firmware_blocked: Some(FIRMWARE_NEEDS_USB.to_string()),
        ..usb.clone()
    };
    // The same Bluetooth link once its check SETTLED on "needs firmware"
    // (a peer that never says hello): Flash is drawn disabled, never the
    // live board pick — the link cannot carry firmware.
    let ble_needs_firmware = PendingLinkView {
        link: DeviceLinkId(10),
        device: DeviceId(110),
        state_label: "Doesn't answer as LightPlayer".to_string(),
        detail: None,
        firmware_face: lpa_studio_core::DeviceFirmwareFace::NoHello,
        ..ble.clone()
    };
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3 tw:sm:grid-cols-2",
            PendingLinkCard { pending: ble, on_action: |_| {} }
            PendingLinkCard { pending: usb, on_action: |_| {} }
            PendingLinkCard { pending: ble_needs_firmware, on_action: |_| {} }
        }
    }
}

#[story(
    description = "The Devices page's Bluetooth settings (BLE M6 S3): the account default password with Show/Hide (shown here), what it is for in one line, and how many passwords this browser remembers with a way to forget them. Local to this browser, never synced."
)]
fn ble_bluetooth_settings() -> Element {
    rsx! {
        div { class: "tw:p-4",
            BluetoothSettingsSection {
                settings: UiDeviceSettingsView {
                    default_password: Some("glitter-otter".to_string()),
                    remembered_passwords: 2,
                },
                on_settings: |_| {},
                on_access: |_| {},
                show_password: true,
            }
        }
    }
}

#[story(
    description = "The project's own Bluetooth list (BLE M6 S5), in its settings: names and what each can do, Add under the list, and where it goes — kept in the library, sent only to your own devices with each push, never in a share or an export."
)]
fn ble_project_bluetooth_list() -> Element {
    rsx! {
        div { class: "tw:max-w-sm tw:p-4",
            ProjectBluetoothSection {
                access: UiProjectAccess {
                    secrets: vec![secret("camp", AccessTier::Play), secret("crew", AccessTier::Edit)],
                    error: None,
                },
                on_access: |_| {},
            }
        }
    }
}

const PANEL_FRAME: &str = "tw:m-3 tw:w-[320px] tw:max-w-[calc(100vw-24px)] tw:rounded-md tw:border tw:border-border-strong tw:bg-card-raised tw:px-3";

fn secret(label: &str, tier: AccessTier) -> UiAccessSecret {
    UiAccessSecret {
        label: label.to_string(),
        tier,
    }
}

fn panel(
    ble_enabled: Option<bool>,
    open: bool,
    secrets: Vec<UiAccessSecret>,
    restart_pending: bool,
) -> UiAccessPanel {
    UiAccessPanel {
        device: DeviceId(7),
        ble_enabled,
        open,
        secrets,
        restart_pending,
        can_restart: true,
        writing: false,
        error: None,
        default_password: Some("glitter-otter".to_string()),
    }
}

/// The catalog choker, running, reached over Bluetooth.
fn ble_card() -> DeviceView {
    DeviceView {
        id: DeviceId(7),
        title: "PLAYFUL choker".to_string(),
        status: DeviceStatus::Ready,
        state_label: "Ready".to_string(),
        detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
        freshness_label: Some("last heard 2 s ago".to_string()),
        identity_label: Some("a0:f2:62:87:b4:8c".to_string()),
        detected_chip: Some("esp32c6".to_string()),
        board_id: Some("seeed/xiao-esp32-c6".to_string()),
        firmware_face: lpa_studio_core::DeviceFirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 abc1234".to_string()),
            wire: lpa_studio_core::DeviceWireVersion::Match,
        },
        remembered_firmware: None,
        degraded: None,
        engine_fps: None,
        loaded_project: DeviceLoadedProject::Running {
            label: "playful-choker".to_string(),
        },
        can_receive_project: true,
        can_remove_project: true,
        activity: None,
        last_outcome: None,
        terminal: Vec::new(),
        terminal_dropped: 0,
        firmware_blocked: Some("Firmware updates need USB".to_string()),
        escapes: vec![DeviceEscape::Disconnect, DeviceEscape::Forget],
    }
}
