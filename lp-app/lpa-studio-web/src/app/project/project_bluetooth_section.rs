//! "Who can use this piece over Bluetooth" (BLE M6 S5): the project's own
//! passwords, stored in the library package as `<project>/.lp/access.json`.
//!
//! It travels to a device with every push and nowhere else: export, share
//! and publish leave it out (M3), and a device never reads it back. The
//! library copy is the source, so this list is exactly what the next push
//! installs.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, AccessTier, NewSecret, UiProjectAccess};

use crate::app::home::access_fields::{
    HELP_CLASS, NameField, PasswordField, SecretRow, TierChoice,
};
use crate::core::quiet_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectBluetoothSection(
    access: UiProjectAccess,
    on_access: EventHandler<AccessCommand>,
    #[props(default)] show_password: bool,
) -> Element {
    let label = use_signal(String::new);
    let password = use_signal(String::new);
    let tier = use_signal(|| AccessTier::Play);
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                "Who can use this piece over Bluetooth"
            }
            if access.secrets.is_empty() {
                p { class: HELP_CLASS, "No passwords of its own yet — the piece's own settings decide." }
            }
            ul { class: "tw:m-0 tw:grid tw:list-none tw:p-0",
                for secret in access.secrets.iter().cloned() {
                    SecretRow {
                        key: "{secret.label}",
                        label: secret.label.clone(),
                        tier: secret.tier,
                        on_remove: move |_| on_access.call(AccessCommand::ProjectSecretRevoke {
                            label: secret.label.clone(),
                        }),
                    }
                }
            }
            form {
                class: "tw:grid tw:gap-1.5",
                onsubmit: move |event| {
                    event.prevent_default();
                    let mut label = label;
                    let mut password = password;
                    on_access.call(AccessCommand::ProjectSecretAdd(NewSecret {
                        label: label.read().trim().to_string(),
                        tier: tier(),
                        password: password.read().clone(),
                    }));
                    label.set(String::new());
                    password.set(String::new());
                },
                div { class: "tw:flex tw:min-w-0 tw:gap-1.5",
                    NameField { value: label, label: "Name (camp, crew…)".to_string() }
                    TierChoice { tier }
                }
                PasswordField { value: password, initially_shown: show_password }
                button { class: quiet_action_class(), r#type: "submit", "Add password" }
            }
            if let Some(error) = access.error.clone() {
                p { class: "tw:m-0 tw:text-xs tw:text-status-error-foreground", "{error}" }
            }
            p { class: HELP_CLASS,
                "Kept in your library. It goes only to your own devices, with each push — never into a share or an export."
            }
        }
    }
}
