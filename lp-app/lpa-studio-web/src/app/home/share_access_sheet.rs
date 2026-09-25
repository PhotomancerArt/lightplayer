//! Share a device (spike §5): generated words to say out loud, and a QR a
//! friend's phone camera opens.
//!
//! Reached from "+ Add a password" at the end of "Who has access". The QR
//! is the share link ([`super::unlock_link`]): our `/unlock` page, with the
//! device's name and the password in the `#fragment`, so the password never
//! reaches a server. Opening it saves the password on that phone and offers
//! Connect.
//!
//! The label defaults to "friends" and the tier to Play: the common case is
//! "let the people at camp change the pattern". "Type my own instead" swaps
//! the words for a password field (the link and QR follow what is typed).
//! Nothing reaches the device until "Add to the device".
//!
//! The words and the link are select-and-copy text, never folded behind a
//! click (the house rule for pasteable text).

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, AccessTier, DeviceAccessChange, DeviceId};

use super::access_fields::{HELP_CLASS, NameField, PasswordField, TEXT_LINK_CLASS, TierChoice};
use super::share_words::share_words;
use super::unlock_link::UnlockLink;
use crate::base::QrCodeSvg;
use crate::core::{outline_action_class, quiet_action_class};

/// The share form. `on_done` returns to the list (after Add, or Back).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ShareAccessSheet(
    device: DeviceId,
    device_name: String,
    on_access: EventHandler<AccessCommand>,
    on_done: EventHandler<()>,
    /// Stories: fixed words instead of fresh ones.
    #[props(default)]
    words: Option<String>,
    /// Stories: start on "Type my own", with this typed.
    #[props(default)]
    typed: Option<String>,
    /// Stories: the product's own origin (never the story server's).
    #[props(default)]
    origin: Option<String>,
    /// A change is being written to the device.
    #[props(default)]
    busy: bool,
) -> Element {
    let mut generated = use_signal(|| words.clone().unwrap_or_else(fresh_words));
    let mut typing = use_signal(|| typed.is_some());
    let typed_password = use_signal(|| typed.clone().unwrap_or_default());
    let label = use_signal(|| "friends".to_string());
    let tier = use_signal(|| AccessTier::Play);
    let origin = origin.unwrap_or_else(this_origin);

    let password = if typing() {
        typed_password.read().clone()
    } else {
        generated.read().clone()
    };
    let link = UnlockLink {
        device_name: device_name.clone(),
        password: password.clone(),
    }
    .url(&origin);
    let can_add = !password.is_empty() && !label.read().trim().is_empty() && !busy;
    let copy_link = link.clone();
    let add_password = password.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:py-1.5",
            div { class: "tw:flex tw:items-center tw:justify-between tw:gap-2",
                h3 { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    "Share {device_name}"
                }
                button {
                    class: TEXT_LINK_CLASS,
                    r#type: "button",
                    onclick: move |_| on_done.call(()),
                    "Back"
                }
            }
            if typing() {
                PasswordField {
                    value: typed_password,
                    initially_shown: true,
                    autofocus: typed.is_none(),
                }
                if !password.is_empty() {
                    ShareQr { link: link.clone(), password: password.clone(), spoken: false }
                }
            } else {
                ShareQr { link: link.clone(), password: password.clone(), spoken: true }
            }
            div { class: "tw:flex tw:gap-2",
                button {
                    class: "{quiet_action_class()} tw:flex-1 tw:justify-center",
                    r#type: "button",
                    disabled: password.is_empty(),
                    onclick: move |_| crate::clipboard::write_text(&copy_link),
                    "Copy link"
                }
                if !typing() {
                    button {
                        class: "{quiet_action_class()} tw:flex-1 tw:justify-center",
                        r#type: "button",
                        onclick: move |_| generated.set(fresh_words()),
                        "New words"
                    }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                NameField { value: label, label: "Name (friends, crew…)".to_string() }
                TierChoice { tier }
            }
            p { class: HELP_CLASS,
                strong { class: "tw:text-muted-foreground", "Play" }
                ": knobs, brightness, patterns. "
                strong { class: "tw:text-muted-foreground", "Edit" }
                ": change the show."
            }
            button {
                class: "{outline_action_class(false)} tw:w-full tw:py-2.5 tw:text-sm",
                r#type: "button",
                disabled: !can_add,
                onclick: move |_| {
                    on_access.call(AccessCommand::Change {
                        device,
                        change: DeviceAccessChange::AddPassword {
                            label: label.read().trim().to_string(),
                            tier: tier(),
                            password: add_password.clone(),
                        },
                    });
                    on_done.call(());
                },
                if busy { "Writing to the device…" } else { "Add to the device" }
            }
            button {
                class: "{TEXT_LINK_CLASS} tw:justify-self-center",
                r#type: "button",
                onclick: move |_| {
                    let was = typing();
                    typing.set(!was);
                },
                if typing() { "Use words instead" } else { "Type my own instead" }
            }
        }
    }
}

/// The QR beside what to say (or, typed, beside the scan hint only).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShareQr(link: String, password: String, spoken: bool) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-3",
            QrCodeSvg { text: link.clone(), size_px: 112, label: Some("QR code: the share link".to_string()) }
            div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                    if spoken {
                        "Scan with a phone camera — it opens LightPlayer and saves the password. Or say it:"
                    } else {
                        "Scan with a phone camera — it opens LightPlayer and saves the password."
                    }
                }
                if spoken {
                    span { class: "tw:select-all tw:[overflow-wrap:anywhere] tw:font-mono tw:text-base tw:font-bold tw:text-strong-foreground",
                        "{password}"
                    }
                }
            }
        }
        // The link itself, whole and selectable (never folded).
        code { class: "tw:block tw:select-all tw:[overflow-wrap:anywhere] tw:rounded-sm tw:bg-card-muted tw:px-1.5 tw:py-1 tw:font-mono tw:text-[11px] tw:leading-snug tw:text-subtle-foreground",
            "{link}"
        }
    }
}

/// Fresh words from the browser's crypto (host builds: fixed, for tests).
fn fresh_words() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let bytes = crate::library_host_opfs::random_bytes();
        share_words([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        share_words([0, 1, 2, 3])
    }
}

/// This page's origin (`https://lightplayer.app`).
fn this_origin() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.location().origin().ok())
            .unwrap_or_else(|| "https://lightplayer.app".to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "https://lightplayer.app".to_string()
    }
}
