//! Bluetooth access stories (plan ble-easy-access, P4; the spike's
//! sections): the card's Connections group, "Who has access", the Unlock
//! sheet, the play-only prompt, the "can now unlock" toast, Share, the
//! friend's page, and Settings — plus the add slot per browser and a
//! Bluetooth link still identifying, which this plan did not change.
//!
//! Every one of these is also captured at the phone width (the story
//! harness's `sm` viewport), which is where G1 reviews them.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessAdded, AccessTier, DeviceEscape, DeviceId, DeviceLinkId, DeviceLoadedProject,
    DeviceStatus, DeviceView, FIRMWARE_NEEDS_USB, PendingLinkView, SecretKind, UiAccessEntry,
    UiAccessPanel, UiDeviceAccess, UiDeviceSettingsView, UiLoginPrompt, UiUnlockOffer,
};
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::AccountAccessInfo;

use crate::app::home::access_added_toast::AccessAddedToast;
use crate::app::home::access_settings_section::AccessSettingsSection;
use crate::app::home::ble_reach::BleReach;
use crate::app::home::browser_identity::BrowserPlatform;
use crate::app::home::device_access_panel::DeviceAccessPanel;
use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
use crate::app::home::devices_page::AddDeviceCard;
use crate::app::home::share_access_sheet::ShareAccessSheet;
use crate::app::home::unlock_link::UnlockLink;
use crate::app::home::unlock_page::UnlockPage;
use crate::app::home::unlock_sheet::UnlockSheet;
use crate::cloud::account_access::AccountAccessState;

// --- 1 · Connections ------------------------------------------------------

#[story(
    description = "The device card's Connections group over USB, Bluetooth ON (the default): a USB row (\"connected\"), a Bluetooth row that is only the icon, the word and a switch, and \"Who has access · 4 ›\" under them, which opens the list. The old \"Bluetooth\" and \"Unlock for edit\" verbs are gone from the Device zone."
)]
fn ble_connections_usb_on() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            DeviceRosterCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), false, typical())),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "Over USB with Bluetooth OFF (left), and just after flipping it on (right): the board reads the switch at boot, so Studio restarts it to apply, and the row says \"Restarting to turn Bluetooth on…\" (the switch waits) until the device says hello again."
)]
fn ble_connections_usb_off_and_restarting() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3 tw:sm:grid-cols-2",
            DeviceRosterCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(false), false, typical())),
                on_action: |_| {},
            }
            DeviceRosterCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), true, typical())),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A device reached over Bluetooth, unlocked at edit by this phone's own key (no screen was shown): the line says \"Unlocked by Yona's iPhone\"; USB reads \"not connected\"; the Bluetooth switch is LOCKED on with \"connected this way — turn off by USB\" (you cannot turn off the radio you are talking over). Firmware and Reset are drawn disabled with \"… need USB\"."
)]
fn ble_connections_over_bluetooth() -> Element {
    let mut panel = panel(Some(true), false, typical());
    panel.over_bluetooth = true;
    panel.can_restart = false;
    let access = UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Unlocked by Yona's iPhone".to_string()),
        unlock: None,
        panel: Some(panel),
    };
    rsx! {
        div { class: CARD_FRAME,
            DeviceRosterCard {
                card: ble_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(access),
                on_action: |_| {},
            }
        }
    }
}

// --- 2 · Who has access ---------------------------------------------------

#[story(
    description = "Who has access, typical: this browser first (marked), your other browser, your account, then a shared play password — only the play entry says \"can play\" and wears the PLAY chip. Then \"Anyone nearby\" with its switch (off). Each row's trash can arms on the first tap. \"+ Add a password\" at the end opens Share; \"USB always gets in.\""
)]
fn ble_who_has_access() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(Some(true), false, typical()),
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "Who has access, crowded, with long names: every row keeps one line and ellipsises its name, never pushing the trash can off the row. Order: this browser, other browsers, accounts, your account's passwords, shared passwords."
)]
fn ble_who_has_access_crowded() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(Some(true), false, crowded()),
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "Who has access with \"Anyone nearby\" ON: anyone in Bluetooth range can play with no password (editing still needs a key). The count on the card's row includes it."
)]
fn ble_who_has_access_open() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(Some(true), true, typical()),
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "One trash can ARMED (the studio's two-tap confirm): red fill, \"Remove\", the quiet 4 s drain under it; the row dims and its second line hides. The can was already as wide as \"Remove\", so nothing moved. A second tap removes; blur or 4 s stands it down."
)]
fn ble_who_has_access_armed() -> Element {
    let entries = typical();
    let armed = entries[1].salt_id;
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(Some(true), false, entries),
                device_name: "PLAYFUL choker".to_string(),
                armed_preview: Some(armed),
                on_access: |_| {},
            }
        }
    }
}

// --- 4 · Unlocking over Bluetooth -----------------------------------------

#[story(
    description = "The Unlock sheet — only when nothing this phone holds matches (the common case has no screen at all). Top: \"This device needs a password to unlock it.\", a \"Device password\" field, Remember on this phone, Not now / Unlock, and the way around it: plug it in by USB once. Bottom: a typed password the device refused, with when it listens again. Never \"log in\", never \"account\"."
)]
fn ble_unlock_sheet() -> Element {
    let prompt = |reason: &str, retry: Option<u64>| UiLoginPrompt {
        device: DeviceId(7),
        device_name: "PLAYFUL choker".to_string(),
        reason: reason.to_string(),
        retry_after_ms: retry,
        busy: false,
    };
    rsx! {
        div { class: "tw:grid tw:gap-4 tw:p-3",
            UnlockSheet {
                prompt: prompt("This device needs a password to unlock it.", None),
                this_word: "phone".to_string(),
                on_access: |_| {},
                inline: true,
            }
            UnlockSheet {
                prompt: prompt("That device password didn't unlock PLAYFUL choker. It will listen again in 4 s.", Some(3_500)),
                this_word: "phone".to_string(),
                on_access: |_| {},
                inline: true,
                typed: Some("s'mores".to_string()),
            }
        }
    }
}

#[story(
    description = "Unlocked for play only (a friend's shared password): the line says \"Unlocked with friends · play\", and where editing would be, one note says what it needs — \"Editing needs an edit password, or plug it in by USB.\" — with \"Enter a password\", which opens the Unlock sheet. A play link sees no \"Who has access\" row (the board lists only at edit)."
)]
fn ble_play_only_prompt() -> Element {
    let access = UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Unlocked with friends · play".to_string()),
        unlock: Some(UiUnlockOffer::PlayOnly),
        panel: None,
    };
    rsx! {
        div { class: CARD_FRAME,
            DeviceRosterCard {
                card: ble_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(access),
                on_action: |_| {},
            }
        }
    }
}

// --- 3 · Plugging in adds this browser ------------------------------------

#[story(
    description = "The toast after plugging a device in by USB (physical connection = access; no prompt): \"Yona's Mac and Yona's account can now unlock PLAYFUL choker over Bluetooth.\" with Undo, which removes exactly those. In the app it sits at the bottom of the page and fades after about ten seconds."
)]
fn ble_access_added_toast() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3",
            AccessAddedToast {
                added: AccessAdded {
                    device: DeviceId(7),
                    names: vec!["Yona's Mac".to_string(), "Yona's account".to_string()],
                    generation: 1,
                },
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_dismiss: |_| {},
                inline: true,
            }
            AccessAddedToast {
                added: AccessAdded {
                    device: DeviceId(7),
                    names: vec!["Chrome on Mac".to_string()],
                    generation: 2,
                },
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_dismiss: |_| {},
                inline: true,
            }
        }
    }
}

// --- 5 · Sharing ------------------------------------------------------------

#[story(
    description = "Share (from \"+ Add a password\"): generated words to say out loud and a real QR — a lightplayer.app/unlock link with the device and password in its #fragment, so the password never reaches a server. Copy link, New words; the label defaults to \"friends\" and the tier to Play; \"Add to the device\"; \"Type my own instead\"."
)]
fn ble_share_words() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            ShareAccessSheet {
                device: DeviceId(7),
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_done: |_| {},
                words: Some("maple-otter-42".to_string()),
                origin: Some("https://lightplayer.app".to_string()),
            }
        }
    }
}

#[story(
    description = "Share with \"Type my own\": a password field (shown) in place of the words; the QR and the link follow what is typed. \"Use words instead\" goes back."
)]
fn ble_share_typed() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            ShareAccessSheet {
                device: DeviceId(7),
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_done: |_| {},
                typed: Some("smores by the fire".to_string()),
                origin: Some("https://lightplayer.app".to_string()),
            }
        }
    }
}

#[story(
    description = "The friend's phone after scanning the QR (lightplayer.app/unlock): no account needed — \"Saved on this phone. Connect to PLAYFUL choker to use it.\" and Connect via Bluetooth (the browser's chooser needs a tap). Right: the same page in iPhone Safari, which has no Web Bluetooth — the add slot's own way forward (Bluefy)."
)]
fn ble_friend_page() -> Element {
    let link = UnlockLink {
        device_name: "PLAYFUL choker".to_string(),
        password: "maple-otter-42".to_string(),
    };
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3 tw:sm:grid-cols-2",
            UnlockPage {
                this_word: "phone".to_string(),
                on_access: |_| {},
                on_action: |_| {},
                link: Some(link.clone()),
                ble_reach: Some(BleReach::Ready),
                page_url: Some("https://lightplayer.app/unlock".to_string()),
            }
            UnlockPage {
                this_word: "phone".to_string(),
                on_access: |_| {},
                on_action: |_| {},
                link: Some(link),
                ble_reach: Some(BleReach::Ios),
                page_url: Some("https://lightplayer.app/unlock".to_string()),
            }
        }
    }
}

// --- 6 · Settings -----------------------------------------------------------

#[story(
    description = "Settings, signed in, with both account passwords set (shown here): this browser's name on your devices (Rename), your account key (Reset account key… is the two-tap confirm), the optional play and edit passwords — Show, Change, trash — and the remembered passwords with Forget them."
)]
fn ble_settings_signed_in_passwords() -> Element {
    rsx! {
        SettingsAs {
            account: AccountAccessState::Ready {
                name: "Yona".to_string(),
                info: account_info(Some("camp-fire-17"), Some("dome-crew-88")),
            },
            passwords_shown: true,
        }
    }
}

#[story(
    description = "Settings, signed in, no account passwords (the default): each reads \"Not set\" with Set. Any browser signed in as you unlocks your devices without them."
)]
fn ble_settings_signed_in_none() -> Element {
    rsx! {
        SettingsAs {
            account: AccountAccessState::Ready {
                name: "Yona".to_string(),
                info: account_info(None, None),
            },
        }
    }
}

#[story(
    description = "Settings, signed out: this browser's own name (\"Chrome on Mac\") and one line for the account: sign in and your devices unlock from any browser you sign in on."
)]
fn ble_settings_signed_out() -> Element {
    rsx! {
        SettingsAs {
            account: AccountAccessState::SignedOut,
            name: "Chrome on Mac".to_string(),
        }
    }
}

// --- unchanged surfaces -----------------------------------------------------

#[story(
    description = "The add slot, per browser (BLE M5 copy, G3 rework). \"Connect a board\", then \"via USB\" and \"via Bluetooth\" — BOTH always drawn; one this browser cannot drive is DISABLED with its reason under it and a way to continue. Chrome/Edge: both live. Brave: Bluetooth disabled, the flag's address as select-and-copy text (a page cannot open brave://). Firefox and desktop Safari: both disabled, both need Chrome or Edge, and this page's address is given ONCE to open there. iPhone Safari (and Chrome on iOS): USB needs a computer; Bluetooth needs Bluefy — a link to it on the App Store, then this page's address to open in it. Bluefy: USB disabled with the address to open on a computer, Bluetooth live. Bluetooth off: turn it on and reload. Never a generic \"connect failed\"."
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
    description = "The add slot in Chrome or Edge on a computer (G3): \"Connect a board\", both buttons live, one full-width column — via USB the spectrum Primary, via Bluetooth the Secondary under it — and \"start a board here\" below."
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
    description = "The add slot in Bluefy on iPhone (G3): Web Bluetooth but no Web Serial. via USB DISABLED with its reason and this page's address to open on a computer; via Bluetooth live."
)]
fn ble_add_slot_bluefy() -> Element {
    rsx! { AddSlotAs { reach: BleReach::Ready, usb: false } }
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

// --- fixtures -------------------------------------------------------------

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

/// The settings section as the Devices page draws it, on a Mac in Chrome.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SettingsAs(
    account: AccountAccessState,
    #[props(default = "Yona's Mac".to_string())] name: String,
    #[props(default)] passwords_shown: bool,
) -> Element {
    rsx! {
        div { class: "tw:p-4",
            AccessSettingsSection {
                settings: UiDeviceSettingsView {
                    browser_name: Some(name),
                    remembered_passwords: 2,
                },
                account,
                platform: BrowserPlatform::Mac,
                browser: "Chrome".to_string(),
                on_access: |_| {},
                on_set_password: |_| {},
                on_reset_key: |_| {},
                passwords_shown,
            }
        }
    }
}

/// A 320px panel, as the popover draws it.
const PANEL_FRAME: &str = "tw:m-3 tw:w-[320px] tw:max-w-[calc(100vw-24px)] tw:rounded-md tw:border tw:border-border-strong tw:bg-card-raised tw:px-3";

/// One card, at most the roster column's width.
const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";

fn entry(
    label: &str,
    kind: SecretKind,
    tier: AccessTier,
    is_this_browser: bool,
    is_account: bool,
    days_ago: u64,
) -> UiAccessEntry {
    UiAccessEntry {
        label: label.to_string(),
        kind,
        tier,
        salt_id: [label.len() as u8; 16],
        is_this_browser,
        is_account,
        // 2026-09-24 12:00 UTC, less the age.
        added_at: Some(1_790_251_200 - days_ago * 86_400),
    }
}

/// This browser, your phone, your account, a shared play password.
fn typical() -> Vec<UiAccessEntry> {
    vec![
        entry(
            "Yona's Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            true,
            false,
            0,
        ),
        entry(
            "Yona's iPhone",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            12,
        ),
        entry(
            "Yona's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            true,
            12,
        ),
        entry(
            "friends",
            SecretKind::Password,
            AccessTier::Play,
            false,
            false,
            4,
        ),
    ]
}

/// Long names, every kind, the account's passwords.
fn crowded() -> Vec<UiAccessEntry> {
    vec![
        entry(
            "friends",
            SecretKind::Password,
            AccessTier::Play,
            false,
            false,
            4,
        ),
        entry(
            "Yona's Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            true,
            false,
            0,
        ),
        entry(
            "Yona's iPhone",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            12,
        ),
        entry(
            "Chrome on Windows (DESKTOP-7Q4K2PL)",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            25,
        ),
        entry(
            "Mireille's Pixel 8 Pro — the one with the cracked screen",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            53,
        ),
        entry(
            "Yona's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            true,
            12,
        ),
        entry(
            "Sam Okonkwo-Lindqvist's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            false,
            53,
        ),
        entry(
            "Yona's play password",
            SecretKind::Password,
            AccessTier::Play,
            false,
            true,
            12,
        ),
        entry(
            "Yona's edit password",
            SecretKind::Password,
            AccessTier::Edit,
            false,
            true,
            12,
        ),
        entry(
            "burning man 2026 — dusty crew",
            SecretKind::Password,
            AccessTier::Play,
            false,
            false,
            31,
        ),
        entry(
            "default",
            SecretKind::Password,
            AccessTier::Edit,
            false,
            false,
            60,
        ),
    ]
}

fn panel(ble_enabled: Option<bool>, open: bool, entries: Vec<UiAccessEntry>) -> UiAccessPanel {
    UiAccessPanel {
        device: DeviceId(7),
        count: entries.len() + usize::from(open),
        entries,
        ble_enabled,
        open,
        restart_pending: false,
        can_restart: true,
        over_bluetooth: false,
        writing: false,
        error: None,
    }
}

fn usb_access(
    ble_enabled: Option<bool>,
    restart_pending: bool,
    entries: Vec<UiAccessEntry>,
) -> UiDeviceAccess {
    let mut panel = panel(ble_enabled, false, entries);
    panel.restart_pending = restart_pending;
    UiDeviceAccess {
        over_bluetooth: false,
        line: None,
        unlock: None,
        panel: Some(panel),
    }
}

fn account_info(play: Option<&str>, edit: Option<&str>) -> AccountAccessInfo {
    AccountAccessInfo {
        key_secret: [1; 32],
        key_salt: [2; 16],
        play_password_salt: [3; 16],
        edit_password_salt: [4; 16],
        play_password: play.map(str::to_string),
        edit_password: edit.map(str::to_string),
        previous_key_salts: Vec::new(),
        updated_at: 0.0,
    }
}

/// The catalog choker, running, on a USB cable.
fn usb_card() -> DeviceView {
    DeviceView {
        firmware_blocked: None,
        ..ble_card()
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
