//! The friend's page (spike §5): where a scanned Share QR lands.
//!
//! `/unlock#<device>&<password>` ([`super::unlock_link`]). The password is
//! read from the fragment at boot — before anything rewrites the address —
//! and the fragment is cleared from the address bar straight away
//! (`history.replaceState`), so it is not left in the tab, its history
//! entry, or a screenshot. The page then saves it in this browser's
//! remembered passwords (no account needed) and offers Connect: Web
//! Bluetooth needs a tap and the browser's own chooser, so it cannot connect
//! on its own. Once connected, the normal silent unlock finds the password
//! and the card says "Unlocked with friends · play".
//!
//! A browser without Web Bluetooth (Brave, Safari, Firefox, iPhone) gets
//! the add slot's own explanation and way forward.

use std::cell::RefCell;

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, DeviceAction, DevicesOp, UiAction};

use super::access_fields::HELP_CLASS;
use super::ble_reach::{BleReach, use_ble_reach};
use super::devices_page::TransportOffer;
use super::reach_note::this_page_url;
use super::unlock_link::{UNLOCK_PATH, UnlockLink};
use crate::base::{StudioIcon, StudioIconName};

thread_local! {
    /// The link read at boot, until the page takes it.
    static CAPTURED: RefCell<Option<UnlockLink>> = const { RefCell::new(None) };
}

/// Read and clear the share link's fragment. Call from `main`, before the
/// app starts (the router's first write would drop the fragment unread).
pub(crate) fn capture_unlock_fragment() {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let location = window.location();
        if location.pathname().ok().as_deref() != Some(UNLOCK_PATH) {
            return;
        }
        let hash = location.hash().unwrap_or_default();
        if hash.is_empty() {
            return;
        }
        let link = UnlockLink::from_fragment(&hash);
        CAPTURED.with(|captured| *captured.borrow_mut() = link);
        let search = location.search().unwrap_or_default();
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(&format!("{UNLOCK_PATH}{search}")),
            );
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = UNLOCK_PATH;
    }
}

/// The page.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn UnlockPage(
    /// "phone", "Mac" — the word after "this".
    this_word: String,
    on_access: EventHandler<AccessCommand>,
    on_action: EventHandler<UiAction>,
    /// Stories: the link as if scanned (the app reads the captured one).
    #[props(default)]
    link: Option<UnlockLink>,
    /// Stories: pin what the Bluetooth half says.
    #[props(default)]
    ble_reach: Option<BleReach>,
    /// Stories: the address the copy lines show.
    #[props(default)]
    page_url: Option<String>,
) -> Element {
    // Take the captured link once, and save its password once.
    let link = use_hook(move || {
        let link = link.or_else(|| CAPTURED.with(|captured| captured.borrow_mut().take()));
        if let Some(link) = &link {
            on_access.call(AccessCommand::RememberPassword(link.password.clone()));
        }
        link
    });
    let asked = use_ble_reach();
    let reach = ble_reach.unwrap_or_else(|| asked());
    let page_url = page_url.unwrap_or_else(this_page_url);
    let connect = DevicesOp::action_for(DeviceAction::AddFromBle);
    let connect = if reach.offers_verb() {
        connect
    } else {
        connect.disabled(reach.note().map_or("", |note| note.reason))
    };
    let Some(link) = link else {
        return rsx! {
            section { class: PAGE_CLASS,
                h2 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "Nothing to save" }
                p { class: HELP_CLASS,
                    "This page saves a device password someone shared with you. The link you opened had none in it — ask them to share it again."
                }
            }
        };
    };
    let device = if link.device_name.trim().is_empty() {
        "the device".to_string()
    } else {
        link.device_name.clone()
    };
    rsx! {
        section { class: PAGE_CLASS,
            div { class: "tw:flex tw:items-start tw:gap-2.5 tw:rounded-md tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-3 tw:py-2.5 tw:text-sm tw:leading-snug tw:text-status-good-foreground",
                span { class: "tw:inline-flex tw:flex-none tw:pt-px",
                    StudioIcon { name: StudioIconName::AccessDone, size: 16 }
                }
                span {
                    "Saved on this {this_word}. Connect to "
                    strong { class: "tw:font-bold", "{device}" }
                    " to use it."
                }
            }
            p { class: HELP_CLASS,
                "Be near it, then connect over Bluetooth. From then on this {this_word} unlocks it whenever you're close."
            }
            div { class: "tw:grid tw:max-w-64 tw:gap-3 tw:text-center",
                TransportOffer {
                    action: connect,
                    note: reach.note(),
                    page_url,
                    on_action: move |action| {
                        on_action.call(action);
                        crate::route_recording::note_route_reason("unlock-connect");
                        crate::router::navigate_push(&crate::router::StudioRoute::Devices);
                    },
                }
            }
        }
    }
}

const PAGE_CLASS: &str =
    "tw:mx-auto tw:grid tw:w-full tw:max-w-md tw:content-start tw:gap-3 tw:py-6";
