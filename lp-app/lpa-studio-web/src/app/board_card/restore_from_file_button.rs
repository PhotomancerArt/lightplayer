//! "Restore from a backup file…" (Decision 11, plan P01), as the firmware
//! bar's details draw it ([`UiDetailPanel::RestoreFromFile`]): a plain
//! button in the details card's row look, paired with a hidden file input —
//! a file dialog cannot be a [`UiAction`], the same reasoning as the project
//! library's own zip Import (`crate::app::home::projects_page`). Core's own
//! offer at the board's `restore-from-file` path exists so the app agent
//! can SEE this is possible (`needs_user_activation`); pressing it for real
//! never reaches that op — picking a file reads its bytes, asks the one
//! question a mismatched backup asks (a native `confirm`), and dispatches
//! [`device_restore_from_file_action`]'s action directly.
//!
//! Behaviour unchanged from today's card (moved out of
//! `device_roster_card.rs`); making it one button is future work.
//!
//! [`UiDetailPanel::RestoreFromFile`]: lpa_studio_core::UiDetailPanel::RestoreFromFile

use dioxus::prelude::*;
use lpa_studio_core::{DeviceId, UiAction, check_backup_file, device_restore_from_file_action};

use crate::base::{StudioIcon, StudioIconName};
use crate::core::menu_item_action_class;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn RestoreFromFileButton(
    device: DeviceId,
    current_base_mac: Option<String>,
    /// The button's classes: the details card's menu row by default.
    #[props(default = menu_item_action_class())]
    class: &'static str,
    on_action: EventHandler<UiAction>,
) -> Element {
    let input_id = format!("restore-from-file-{}", device.0);
    let picked = restore_from_file_handler(device, current_base_mac, on_action);
    rsx! {
        button {
            class,
            r#type: "button",
            title: "Pick a backup .zip from your computer and replace this board's files with it.",
            onclick: {
                let input_id = input_id.clone();
                move |_| open_file_picker(&input_id)
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                StudioIcon { name: StudioIconName::Upload, size: 14 }
            }
            span { "Restore from a backup file…" }
        }
        input {
            class: "tw:hidden",
            id: "{input_id}",
            r#type: "file",
            accept: ".zip",
            onchange: move |event| picked(event.files()),
        }
    }
}

/// Open the hidden file input `input_id` (a click on it, as a user's
/// click would).
#[cfg(target_arch = "wasm32")]
pub(crate) fn open_file_picker(input_id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(input_id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        input.click();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_file_picker(_input_id: &str) {}

/// Read the picked `.zip`, ask the one question a mismatched backup ever
/// asks (a native confirm — the same `window.confirm` idiom
/// `unsaved_gate.rs` uses for a quick sanity check ahead of a destructive
/// action), and dispatch the real import. A refused archive is said in
/// words, never a raw code, through a native alert: this click never goes
/// near `on_action`'s generic dispatch until the file has already checked
/// out.
fn restore_from_file_handler(
    device: DeviceId,
    current_base_mac: Option<String>,
    on_action: EventHandler<UiAction>,
) -> impl Fn(Vec<dioxus::html::FileData>) + Clone + 'static {
    move |files: Vec<dioxus::html::FileData>| {
        let current_base_mac = current_base_mac.clone();
        spawn(async move {
            let Some(file) = files.into_iter().next() else {
                return;
            };
            let name = file.name();
            if !name.to_lowercase().ends_with(".zip") {
                say(&format!("{name} is not a backup .zip"));
                return;
            }
            let bytes = match file.read_bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(error) => {
                    log::warn!("restore from file: could not read {name}: {error}");
                    say(&format!("{name} could not be read"));
                    return;
                }
            };
            match check_backup_file(&bytes, current_base_mac.as_deref()) {
                Err(message) => say(&message),
                Ok(Some(mismatch)) => {
                    if !confirm(&format!("{mismatch} Restore it onto this board anyway?")) {
                        return;
                    }
                    on_action.call(device_restore_from_file_action(device, name, bytes));
                }
                Ok(None) => {
                    on_action.call(device_restore_from_file_action(device, name, bytes));
                }
            }
        });
    }
}

/// One native dialog, in words — never a raw code (plan P01). Host builds
/// (and any context without a `window`) just log it.
#[cfg(target_arch = "wasm32")]
pub(crate) fn say(message: &str) {
    let _ = web_sys::window().and_then(|window| window.alert_with_message(message).ok());
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn say(message: &str) {
    log::warn!("{message}");
}

/// The one question a mismatched backup ever asks (ease over ceremony: no
/// second confirmation stacks on top of it). Host builds proceed — the gate
/// is a browser affordance, same reasoning as `confirm_discarding_unsaved`.
#[cfg(target_arch = "wasm32")]
fn confirm(message: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.confirm_with_message(message).ok())
        .unwrap_or(true)
}

#[cfg(not(target_arch = "wasm32"))]
fn confirm(_message: &str) -> bool {
    true
}
