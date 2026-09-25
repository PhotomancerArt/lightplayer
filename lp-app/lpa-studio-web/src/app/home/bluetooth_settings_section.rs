//! The Devices page's Bluetooth settings (BLE M6 S3): the account default
//! password, and forgetting the ones this browser remembers.
//!
//! It lives on the Devices page, under the roster, because that is where
//! Bluetooth is turned on for a piece — the one place the default is used
//! besides logging in. (The header's settings popover is the AI assistant's.)
//! Local to this browser, never synced (PQ8).

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, SettingsCommand, UiDeviceSettingsView};

use super::access_fields::{HELP_CLASS, PasswordField};
use super::section_title_class;
use crate::core::quiet_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BluetoothSettingsSection(
    settings: UiDeviceSettingsView,
    on_settings: EventHandler<SettingsCommand>,
    on_access: EventHandler<AccessCommand>,
    /// Stories: show the password.
    #[props(default)]
    show_password: bool,
) -> Element {
    let password = use_signal(|| settings.default_password.clone().unwrap_or_default());
    let saved = settings.default_password.clone().unwrap_or_default();
    let dirty = *password.read() != saved;
    let remembered = settings.remembered_passwords;
    rsx! {
        section { class: "tw:grid tw:max-w-md tw:gap-2",
            h2 { class: section_title_class(), "Bluetooth" }
            form {
                class: "tw:grid tw:gap-1.5",
                onsubmit: move |event| {
                    event.prevent_default();
                    let typed = password.read().clone();
                    on_settings.call(SettingsCommand::SetDeviceDefaultPassword(
                        (!typed.is_empty()).then_some(typed),
                    ));
                },
                label { class: "tw:text-sm tw:font-semibold tw:text-strong-foreground", "Default device password" }
                div { class: "tw:flex tw:min-w-0 tw:gap-1.5",
                    div { class: "tw:min-w-0 tw:flex-1",
                        PasswordField { value: password, label: "Default device password".to_string(), initially_shown: show_password }
                    }
                    button { class: quiet_action_class(), r#type: "submit", disabled: !dirty, "Save" }
                }
                p { class: HELP_CLASS,
                    "Pre-filled when you turn on Bluetooth for a piece, and tried first when one asks. Friends who know it can use your pieces at the level you chose then."
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
                span { class: "tw:text-xs tw:text-subtle-foreground", "{remembered_line(remembered)}" }
                if remembered > 0 {
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        onclick: move |_| on_access.call(AccessCommand::ForgetRememberedPasswords),
                        "Forget remembered passwords"
                    }
                }
            }
        }
    }
}

fn remembered_line(count: usize) -> String {
    match count {
        0 => "No passwords remembered on this browser.".to_string(),
        1 => "1 password remembered on this browser.".to_string(),
        n => format!("{n} passwords remembered on this browser."),
    }
}
