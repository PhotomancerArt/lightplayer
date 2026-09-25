//! The Devices page's Bluetooth settings: forgetting the passwords this
//! browser remembers.
//!
//! The account default device password is gone (plan D10): plugging a
//! device in by USB installs this browser's key, so nothing needs a default.
//! P4 adds this browser's name and the account's passwords here. Local to
//! this browser, never synced (PQ8).

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, UiDeviceSettingsView};

use super::section_title_class;
use crate::core::quiet_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BluetoothSettingsSection(
    settings: UiDeviceSettingsView,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    let remembered = settings.remembered_passwords;
    rsx! {
        section { class: "tw:grid tw:max-w-md tw:gap-2",
            h2 { class: section_title_class(), "Bluetooth" }
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
