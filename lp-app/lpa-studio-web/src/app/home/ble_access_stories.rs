//! Bluetooth access stories (plan ble-easy-access, P4; the access panel is
//! spike `access-panel-tidy` concept 4B): the card's Connections group, the
//! access panel in its four states and its key list, the Unlock sheet, the
//! play-only prompt, the "can now unlock" toast, the friend's page, and
//! Settings — plus the add slot per browser and a Bluetooth link still
//! identifying.
//!
//! Every one of these is also captured at the phone width (the story
//! harness's `sm` viewport), which is where G1 reviews them.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessAdded, AccessTier, DeviceEscape, DeviceId, DeviceLinkId, DeviceLoadedProject,
    DeviceStatus, DeviceView, DroppedKey, FIRMWARE_NEEDS_USB, OpenTo, PendingLinkView, SecretKind,
    UiAccessPanel, UiDeviceAccess, UiDeviceSettingsView, UiKeyGroup, UiLoginPrompt, UiPasswordLine,
    UiUnlockOffer,
};
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::AccountAccessInfo;

use crate::app::home::access_added_toast::AccessAddedToast;
use crate::app::home::access_settings_section::AccessSettingsSection;
use crate::app::home::ble_reach::BluetoothReach;
use crate::app::home::browser_identity::BrowserPlatform;
use crate::app::home::device_access_panel::DeviceAccessPanel;
use crate::app::home::device_offer_story_fixtures::{
    StoryDeviceCard, StoryPendingCard, add_slot_tree,
};
use crate::app::home::devices_page::AddDeviceCard;
use crate::app::home::unlock_link::UnlockLink;
use crate::app::home::unlock_page::UnlockPage;
use crate::app::home::unlock_sheet::UnlockSheet;
use crate::cloud::account_access::AccountAccessState;
use crate::core::OffersProvider;

// --- 1 · Connections ------------------------------------------------------

#[story(
    description = "The device card's Connections group over USB, Bluetooth ON (the default): a USB row (\"connected\"), a Bluetooth row that is only the icon, the word and a switch, and \"Access · open ›\" under them (warning-tinted: a new board is open to anyone nearby, for now), which opens the access panel. The old \"Bluetooth\" and \"Unlock for edit\" verbs are gone from the Device zone."
)]
fn ble_connections_usb_on() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), false)),
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
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(false), false)),
                on_action: |_| {},
            }
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), true)),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A device reached over Bluetooth, unlocked at edit by this phone's own key (no screen was shown): the line says \"Unlocked by Yona's iPhone\"; USB reads \"not connected\"; the Bluetooth switch is LOCKED on with \"connected this way — turn off by USB\" (you cannot turn off the radio you are talking over). Firmware and Reset are drawn disabled with \"… need USB\"."
)]
fn ble_connections_over_bluetooth() -> Element {
    let mut panel = panel(
        OpenTo::Play,
        UiPasswordLine::Anyone,
        UiPasswordLine::Shown("maple-otter-42".to_string()),
        keys_typical(),
    );
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
            StoryDeviceCard {
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

// --- 2 · Access ------------------------------------------------------------

#[story(
    description = "Access on a NEW board (the default, for now), as a detail card: the ACCESS section says what it is in one sentence, then Author is Anyone, so Play is greyed and reads \"follows Author\". The second section is one line: \"Your browsers & account · always get in · 14 of 16 · added by USB\" (closed)."
)]
fn ble_access_wide_open() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(OpenTo::Edit, UiPasswordLine::FollowsAuthor, UiPasswordLine::Anyone, keys_typical()),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "Play open, author locked: Play is Anyone (\"no password\"); Author is Password, its box showing the password this browser set (monospace, selected whole when you click in, so typing replaces it; ↻ rolls another; it saves when you click away)."
)]
fn ble_access_play_open() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(
                    OpenTo::Play,
                    UiPasswordLine::Anyone,
                    UiPasswordLine::Shown("maple-otter-42".to_string()),
                    keys_typical(),
                ),
                on_access: |_| {},
            }
        }
    }
}

#[story(description = "Both locked: two passwords, each in its own box.")]
fn ble_access_both_locked() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(
                    OpenTo::Nobody,
                    UiPasswordLine::Shown("camp-glow-17".to_string()),
                    UiPasswordLine::Shown("maple-otter-42".to_string()),
                    keys_typical(),
                ),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "Author set from another browser: the board keeps only a derived key, so the box is empty with \"type a new one\" and one line under it — \"Set from another browser, so it can't be shown — type a new one to replace it.\""
)]
fn ble_access_set_elsewhere() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(
                    OpenTo::Nobody,
                    UiPasswordLine::Shown("camp-glow-17".to_string()),
                    UiPasswordLine::SetElsewhere,
                    keys_typical(),
                ),
                on_access: |_| {},
            }
        }
    }
}

#[story(
    description = "Just after Author went to Anyone with Play on Password: Play followed, and the panel says so in one line — \"Author is open now, so play is too.\""
)]
fn ble_access_author_opened() -> Element {
    let mut panel = panel(
        OpenTo::Edit,
        UiPasswordLine::FollowsAuthor,
        UiPasswordLine::Anyone,
        keys_typical(),
    );
    panel.notice = Some("Author is open now, so play is too.".to_string());
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel { panel, on_access: |_| {} }
        }
    }
}

#[story(
    description = "The keys open, on a FULL desk board (16 of 16): this browser first, then \"Brave on Mac ×11\" — one per dev-server origin — over its date span, then the phone and the account. A new key took the oldest browser's place, and the panel says which."
)]
fn ble_access_keys_full() -> Element {
    let mut panel = panel(
        OpenTo::Play,
        UiPasswordLine::Anyone,
        UiPasswordLine::Shown("maple-otter-42".to_string()),
        keys_full(),
    );
    panel.notice = Some("To make room, an older Brave on Mac was dropped.".to_string());
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel { panel, on_access: |_| {}, keys_open_preview: true }
        }
    }
}

#[story(
    description = "One group's trash can ARMED (the studio's two-tap confirm): red fill, \"Remove\", the quiet 4 s drain; the row dims. A second tap removes all eleven \"Brave on Mac\" keys at once."
)]
fn ble_access_keys_armed() -> Element {
    let keys = keys_full();
    let armed = keys[1].salts[0];
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(OpenTo::Edit, UiPasswordLine::FollowsAuthor, UiPasswordLine::Anyone, keys),
                on_access: |_| {},
                keys_open_preview: true,
                armed_preview: Some(armed),
            }
        }
    }
}

#[story(
    description = "The keys open with long names and every kind: each row keeps one line and ellipsises its name, never pushing the trash can off the row. Order: this browser, other browsers, your account and its passwords, then another account."
)]
fn ble_access_keys_crowded() -> Element {
    rsx! {
        div { class: PANEL_FRAME,
            DeviceAccessPanel {
                panel: panel(OpenTo::Nobody, UiPasswordLine::NotSet, UiPasswordLine::SetElsewhere, keys_crowded()),
                on_access: |_| {},
                keys_open_preview: true,
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
    description = "Unlocked for play only (a friend's shared password): the line says \"Unlocked with friends · play\", and where editing would be, one note says what it needs — \"Authoring needs an author password, or plug it in by USB.\" — with \"Enter a password\", which opens the Unlock sheet. A play link sees no \"Access\" row (the board lists only at author)."
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
            StoryDeviceCard {
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

#[story(
    description = "Reached over Bluetooth, and nothing this browser holds unlocked it: the board answers only its hello and the unlock, so the card does not guess what it runs. The picture slot says it is locked and what to do — \"Locked — Unlock it to see what it runs.\" — the project line is empty with no \"Put it on the board\", and the device line ends in \"Unlock\", which opens the sheet."
)]
fn ble_card_locked() -> Element {
    let access = UiDeviceAccess {
        over_bluetooth: true,
        line: Some("Needs a device password".to_string()),
        unlock: Some(UiUnlockOffer::Locked),
        panel: None,
    };
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: DeviceView {
                    loaded_project: DeviceLoadedProject::Empty,
                    can_remove_project: false,
                    ..ble_card()
                },
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: None,
                access: Some(access),
                on_action: |_| {},
            }
        }
    }
}

// --- 3 · Plugging in adds this browser ------------------------------------

#[story(
    description = "The toast after plugging a device in by USB (physical connection = access; no prompt): \"Yona's Mac and Yona's account can now unlock PLAYFUL choker over Bluetooth.\" with Undo, which removes exactly those. Bottom: a full device, where the new key took the oldest browser's place — \"To make room, an older Brave on Mac was dropped.\" In the app it sits at the bottom of the page and fades after about ten seconds."
)]
fn ble_access_added_toast() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3 tw:p-3",
            AccessAddedToast {
                added: AccessAdded {
                    device: DeviceId(7),
                    names: vec!["Yona's Mac".to_string(), "Yona's account".to_string()],
                    dropped: Vec::new(),
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
                    dropped: Vec::new(),
                    generation: 2,
                },
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_dismiss: |_| {},
                inline: true,
            }
            AccessAddedToast {
                added: AccessAdded {
                    device: DeviceId(7),
                    names: vec!["Brave on Mac".to_string()],
                    dropped: vec![DroppedKey {
                        label: "Brave on Mac".to_string(),
                        added_at: Some(1_790_251_200),
                    }],
                    generation: 3,
                },
                device_name: "PLAYFUL choker".to_string(),
                on_access: |_| {},
                on_dismiss: |_| {},
                inline: true,
            }
        }
    }
}

// --- 5 · A shared link ---------------------------------------------------

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
            OffersProvider { offers: add_slot_tree(true, BluetoothReach::Ready),
                UnlockPage {
                    this_word: "phone".to_string(),
                    on_access: |_| {},
                    on_action: |_| {},
                    link: Some(link.clone()),
                    ble_reach: Some(BluetoothReach::Ready),
                    page_url: Some("https://lightplayer.app/unlock".to_string()),
                }
            }
            OffersProvider { offers: add_slot_tree(false, BluetoothReach::Ios),
                UnlockPage {
                    this_word: "phone".to_string(),
                    on_access: |_| {},
                    on_action: |_| {},
                    link: Some(link),
                    ble_reach: Some(BluetoothReach::Ios),
                    page_url: Some("https://lightplayer.app/unlock".to_string()),
                }
            }
        }
    }
}

// --- 6 · Settings -----------------------------------------------------------

#[story(
    description = "Settings, signed in, with both account passwords set (shown here): this browser's name on your devices (Rename), your account key (Reset account key… is the two-tap confirm), the optional play and author passwords — Show, Change, trash — and the remembered passwords with Forget them."
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
    rsx! { AddSlotAs { reach: BluetoothReach::Ready, usb: true } }
}

#[story(
    description = "The add slot in Brave (G3): via USB live; via Bluetooth DISABLED — \"Brave keeps Bluetooth behind a flag.\" — with the flag's address as select-and-copy text, because a page cannot open a brave:// link."
)]
fn ble_add_slot_brave() -> Element {
    rsx! { AddSlotAs { reach: BluetoothReach::Brave, usb: true } }
}

#[story(
    description = "The add slot in Firefox (G3): both buttons DISABLED — USB needs Chrome or Edge on a computer, Bluetooth needs Chrome or Edge — and this page's address, once, as select-and-copy text to open there."
)]
fn ble_add_slot_firefox() -> Element {
    rsx! { AddSlotAs { reach: BluetoothReach::Firefox, usb: false } }
}

#[story(
    description = "The add slot in Safari on iPhone — and Chrome on iPhone, which is the same WebKit (G3): via USB DISABLED (it needs a computer, with this page's address to open there); via Bluetooth DISABLED with the way through: \"Get Bluefy on the App Store\", then this page's address to open in Bluefy."
)]
fn ble_add_slot_iphone_safari() -> Element {
    rsx! { AddSlotAs { reach: BluetoothReach::Ios, usb: false } }
}

#[story(
    description = "The add slot in Bluefy on iPhone (G3): Web Bluetooth but no Web Serial. via USB DISABLED with its reason and this page's address to open on a computer; via Bluetooth live."
)]
fn ble_add_slot_bluefy() -> Element {
    rsx! { AddSlotAs { reach: BluetoothReach::Ready, usb: false } }
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
            StoryPendingCard { pending: ble, on_action: |_| {} }
            StoryPendingCard { pending: usb, on_action: |_| {} }
            StoryPendingCard { pending: ble_needs_firmware, on_action: |_| {} }
        }
    }
}

// --- fixtures -------------------------------------------------------------

/// Each browser as the real ones pair Bluetooth reach with Web Serial.
const ADD_SLOT_BROWSERS: [(&str, BluetoothReach, bool); 7] = [
    ("Chrome / Edge", BluetoothReach::Ready, true),
    ("Brave", BluetoothReach::Brave, true),
    ("Firefox", BluetoothReach::Firefox, false),
    ("Safari (Mac)", BluetoothReach::Safari, false),
    ("iPhone Safari / Chrome", BluetoothReach::Ios, false),
    ("Bluefy (iPhone)", BluetoothReach::Ready, false),
    ("Chrome, Bluetooth off", BluetoothReach::Off, true),
];

/// The add slot pinned to one browser's answers, with the product's own
/// address in its copy lines (never the story server's).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddSlotAs(reach: BluetoothReach, usb: bool) -> Element {
    rsx! {
        div { class: "tw:p-3",
            OffersProvider { offers: add_slot_tree(usb, reach),
                AddDeviceCard {
                    ble_reach: Some(reach),
                    usb_available: usb,
                    page_url: Some("https://lightplayer.app/devices".to_string()),
                    on_action: |_| {},
                }
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
const PANEL_FRAME: &str = "tw:m-3 tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:text-sm tw:text-muted-foreground ux-glass-panel";

/// One card, at most the roster column's width.
const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";

/// One key group: `n` entries added from `first` to `last` days before
/// 2026-10-02.
fn group(
    label: &str,
    kind: SecretKind,
    tier: AccessTier,
    is_this_browser: bool,
    is_account: bool,
    n: u8,
    first: u64,
    last: u64,
) -> UiKeyGroup {
    // 2026-10-02 12:00 UTC.
    const NOW: u64 = 1_790_942_400;
    let seed = label.len() as u8;
    UiKeyGroup {
        label: label.to_string(),
        kind,
        tier,
        salts: (0..n)
            .map(|i| [seed.wrapping_add(i.wrapping_mul(17)); 16])
            .collect(),
        is_this_browser,
        is_account,
        first_added: Some(NOW - first * 86_400),
        last_added: Some(NOW - last * 86_400),
    }
}

/// This browser, eleven dev-server origins, your phone, your account.
fn keys_typical() -> Vec<UiKeyGroup> {
    vec![
        group(
            "Yona's Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            true,
            false,
            1,
            0,
            0,
        ),
        group(
            "Brave on Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            11,
            6,
            0,
        ),
        group(
            "Bluefy on iPhone",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            1,
            8,
            8,
        ),
        group(
            "Yona's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            true,
            1,
            12,
            12,
        ),
    ]
}

/// A full desk board: 16 of 16 with the two passwords.
fn keys_full() -> Vec<UiKeyGroup> {
    vec![
        group(
            "Yona's Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            true,
            false,
            1,
            0,
            0,
        ),
        group(
            "Brave on Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            11,
            6,
            0,
        ),
        group(
            "Bluefy on iPhone",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            1,
            8,
            8,
        ),
        group(
            "Yona's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            true,
            1,
            12,
            12,
        ),
        group(
            "Yona's play password",
            SecretKind::Password,
            AccessTier::Play,
            false,
            true,
            1,
            12,
            12,
        ),
    ]
}

/// Long names, every kind, the account's passwords.
fn keys_crowded() -> Vec<UiKeyGroup> {
    vec![
        group(
            "Yona's Mac",
            SecretKind::Browser,
            AccessTier::Edit,
            true,
            false,
            1,
            0,
            0,
        ),
        group(
            "Yona's iPhone",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            1,
            12,
            12,
        ),
        group(
            "Chrome on Windows (DESKTOP-7Q4K2PL)",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            3,
            25,
            4,
        ),
        group(
            "Mireille's Pixel 8 Pro — the one with the cracked screen",
            SecretKind::Browser,
            AccessTier::Edit,
            false,
            false,
            1,
            53,
            53,
        ),
        group(
            "Yona's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            true,
            1,
            12,
            12,
        ),
        group(
            "Yona's play password",
            SecretKind::Password,
            AccessTier::Play,
            false,
            true,
            1,
            12,
            12,
        ),
        group(
            "Yona's author password",
            SecretKind::Password,
            AccessTier::Edit,
            false,
            true,
            1,
            12,
            12,
        ),
        group(
            "Sam Okonkwo-Lindqvist's account",
            SecretKind::Account,
            AccessTier::Edit,
            false,
            false,
            1,
            53,
            53,
        ),
    ]
}

fn panel(
    open: OpenTo,
    play: UiPasswordLine,
    author: UiPasswordLine,
    keys: Vec<UiKeyGroup>,
) -> UiAccessPanel {
    let passwords = [&play, &author]
        .into_iter()
        .filter(|line| {
            matches!(
                line,
                UiPasswordLine::Shown(_) | UiPasswordLine::SetElsewhere
            )
        })
        .count();
    UiAccessPanel {
        open,
        play,
        author,
        used: keys.iter().map(UiKeyGroup::count).sum::<usize>() + passwords,
        keys,
        ble_enabled: Some(true),
        ..UiAccessPanel::reading(DeviceId(7))
    }
}

fn usb_access(ble_enabled: Option<bool>, restart_pending: bool) -> UiDeviceAccess {
    let mut panel = panel(
        OpenTo::Edit,
        UiPasswordLine::FollowsAuthor,
        UiPasswordLine::Anyone,
        keys_typical(),
    );
    panel.ble_enabled = ble_enabled;
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
        link_counters: None,
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
