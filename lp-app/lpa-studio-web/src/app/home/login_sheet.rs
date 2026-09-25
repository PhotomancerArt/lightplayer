//! The password sheet (BLE M6 S2): a piece asked for a password Studio did
//! not have, refused the ones it tried, or refused an edit at play.
//!
//! Page-level, so it rises over whatever the user is looking at — the
//! devices page after "via Bluetooth", or Play when a piece dropped
//! and came back. On a phone it is a bottom sheet (the thumb's reach); on a
//! wide window a centred card. It never appears for a piece that let
//! Studio in with a password it already knew.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, UiLoginPrompt};

use super::access_fields::{HELP_CLASS, PasswordField};
use crate::core::{outline_action_class, quiet_action_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LoginSheet(
    prompt: UiLoginPrompt,
    on_access: EventHandler<AccessCommand>,
    /// Stories: a capture pins the sheet in its box instead of the viewport.
    #[props(default)]
    inline: bool,
    /// Stories: pre-type a password (a capture cannot type).
    #[props(default)]
    typed: Option<String>,
) -> Element {
    let device = prompt.device;
    let password = use_signal(|| typed.clone().unwrap_or_default());
    let mut remember = use_signal(|| true);
    let busy = prompt.busy;
    let submit_label = if busy { "Unlocking…" } else { "Unlock" };
    let frame_class = if inline {
        INLINE_FRAME_CLASS
    } else {
        OVERLAY_CLASS
    };
    rsx! {
        div { class: frame_class,
            role: "dialog",
            aria_modal: "true",
            aria_label: "Unlock {prompt.device_name}",
            form {
                class: SHEET_CLASS,
                onsubmit: move |event| {
                    event.prevent_default();
                    let typed = password.read().clone();
                    if typed.is_empty() {
                        return;
                    }
                    on_access.call(AccessCommand::SubmitPassword {
                        device,
                        password: typed,
                        remember: remember(),
                    });
                },
                h2 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground",
                    "Unlock {prompt.device_name}"
                }
                p { class: "tw:m-0 tw:text-sm tw:leading-snug tw:text-muted-foreground", "{prompt.reason}" }
                PasswordField { value: password, autofocus: true }
                label { class: "tw:flex tw:items-center tw:gap-2 tw:text-sm tw:text-strong-foreground",
                    input {
                        r#type: "checkbox",
                        checked: remember(),
                        onchange: move |event| remember.set(event.checked()),
                    }
                    "Remember on this browser"
                }
                p { class: HELP_CLASS,
                    "Studio tries remembered device passwords first on the next piece that asks."
                }
                div { class: "tw:flex tw:items-center tw:justify-end tw:gap-2 tw:pt-1",
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        onclick: move |_| on_access.call(AccessCommand::Dismiss { device }),
                        "Not now"
                    }
                    button {
                        class: outline_action_class(false),
                        r#type: "submit",
                        disabled: busy,
                        "{submit_label}"
                    }
                }
            }
        }
    }
}

/// The viewport overlay: a dim backdrop, the sheet at the bottom on a
/// phone and centred from `sm` up.
const OVERLAY_CLASS: &str = "tw:fixed tw:inset-0 tw:z-50 tw:flex tw:items-end tw:justify-center tw:bg-black/50 tw:sm:items-center tw:sm:p-6";

/// The stories' frame: the same sheet, in flow.
const INLINE_FRAME_CLASS: &str = "tw:flex tw:justify-center tw:bg-black/50 tw:p-3";

const SHEET_CLASS: &str = "tw:grid tw:w-full tw:max-w-sm tw:gap-3 tw:rounded-t-lg tw:border tw:border-border-strong tw:bg-card-raised tw:p-4 tw:pb-6 tw:shadow-2xl tw:sm:rounded-lg tw:sm:pb-4";
