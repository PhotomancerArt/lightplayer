//! The Unlock sheet (spike §4): a device over Bluetooth that nothing this
//! browser holds unlocks — none of its keys, none of the account's, no
//! remembered password — or one unlocked for play, asked to edit.
//!
//! The common case has no screen at all: keys are matched by salt and the
//! card just says "Unlocked by <name>". So when this sheet does rise it
//! says plainly what is needed — "This device needs a password to unlock
//! it." — in a "Device password" field, never "log in" and never
//! "account": a person must not type their account password here. The
//! footer says the way around it: plug the device in by USB once, and this
//! browser's key goes on it.
//!
//! Page-level, so it rises over whatever the user is looking at. On a phone
//! it is a bottom sheet (the thumb's reach); on a wide window a centred
//! card.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, UiLoginPrompt};

use super::access_fields::{HELP_CLASS, PasswordField};
use crate::base::{StudioIcon, StudioIconName};
use crate::core::{outline_action_class, quiet_action_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn UnlockSheet(
    prompt: UiLoginPrompt,
    /// "phone", "Mac" — the word after "this" (the page's platform).
    this_word: String,
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
                div { class: "tw:mx-auto tw:-mt-1 tw:h-1 tw:w-9 tw:rounded-full tw:bg-border-strong tw:sm:hidden" }
                h2 { class: "tw:m-0 tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-base tw:font-bold tw:text-strong-foreground",
                    span { class: "tw:inline-flex tw:flex-none",
                        StudioIcon { name: StudioIconName::AccessLocked, size: 17 }
                    }
                    span { class: "tw:min-w-0 tw:truncate", "Unlock {prompt.device_name}" }
                }
                p { class: "tw:m-0 tw:text-sm tw:leading-snug tw:text-muted-foreground", "{prompt.reason}" }
                PasswordField { value: password, autofocus: !inline }
                label { class: "tw:flex tw:items-center tw:gap-2 tw:text-sm tw:text-strong-foreground",
                    input {
                        r#type: "checkbox",
                        checked: remember(),
                        onchange: move |event| remember.set(event.checked()),
                    }
                    "Remember on this {this_word}"
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
                p { class: "{HELP_CLASS} tw:border-t tw:border-border-muted tw:pt-2.5",
                    "Yours? Plug it in by USB once and this {this_word} unlocks it from then on."
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

const SHEET_CLASS: &str = "tw:grid tw:w-full tw:max-w-sm tw:gap-3 tw:rounded-t-xl tw:border tw:border-b-0 tw:border-border-strong tw:bg-card-raised tw:p-4 tw:pb-[calc(1.5rem+env(safe-area-inset-bottom,0px))] tw:shadow-2xl tw:sm:rounded-lg tw:sm:border-b tw:sm:pb-4";
