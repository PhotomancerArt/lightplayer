//! The device access panel (BLE M6 S4): who may reach this piece over
//! Bluetooth.
//!
//! It opens from the device card's own verb row ("Bluetooth") into a panel
//! in the top layer, so the card keeps its fixed height. It is offered over
//! USB — the trusted link — and over a Bluetooth login at edit.
//!
//! Everything here writes the piece's device store (`/.lp/access.json`),
//! which no link can read back. So the list is **what this browser wrote**,
//! and the panel says so, plus the recovery: saving from here replaces
//! anything another browser added.
//!
//! | state | what it says |
//! |---|---|
//! | never written from here | "Studio hasn't set Bluetooth on this piece from this browser …" + Turn on |
//! | off | "Off — reach it by USB" + Turn on |
//! | on, locked | the passwords, Add (at the list's end), open switch, Turn off |
//! | on, open | the same, with "anyone nearby can play" said first |
//! | switched since the last restart | "Restart to apply" + Restart now (USB) |

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, AccessTier, DeviceAccessChange, NewSecret, UiAccessPanel};

use super::access_fields::{
    HELP_CLASS, NameField, PasswordField, SecretRow, TEXT_BUTTON_CLASS, TierChoice,
};
use crate::core::{outline_action_class, quiet_action_class};

/// The panel body (the popover's content, and the stories' subject).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceAccessPanel(
    panel: UiAccessPanel,
    on_access: EventHandler<AccessCommand>,
    /// Stories: show the password fields' text.
    #[props(default)]
    show_passwords: bool,
) -> Element {
    let device = panel.device;
    let busy = panel.writing;
    let change =
        move |change: DeviceAccessChange| on_access.call(AccessCommand::Change { device, change });
    let on = panel.ble_enabled == Some(true);
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:py-1.5",
            h3 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "Bluetooth" }
            p { class: "tw:m-0 tw:text-sm tw:leading-snug tw:text-muted-foreground", "{state_sentence(&panel)}" }

            if panel.restart_pending {
                div { class: "tw:grid tw:gap-2 tw:rounded tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-3 tw:py-2",
                    p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-warning-foreground",
                        {restart_sentence(&panel)}
                    }
                    if panel.can_restart {
                        button {
                            class: outline_action_class(false),
                            r#type: "button",
                            disabled: busy,
                            onclick: move |_| on_access.call(AccessCommand::Restart { device }),
                            "Restart now"
                        }
                    }
                }
            }

            if on {
                OnSection { panel: panel.clone(), on_access, show_passwords }
            } else {
                TurnOnForm {
                    default_password: panel.default_password.clone(),
                    busy,
                    show_passwords,
                    on_change: change,
                }
            }

            if busy {
                p { class: HELP_CLASS, "Writing to the piece…" }
            }
            if let Some(error) = panel.error.clone() {
                p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground", "{error}" }
            }
        }
    }
}

/// Bluetooth is on: the passwords this browser wrote, Add at the end of
/// the list, the open switch, Replace all, and Turn off.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OnSection(
    panel: UiAccessPanel,
    on_access: EventHandler<AccessCommand>,
    show_passwords: bool,
) -> Element {
    let device = panel.device;
    let busy = panel.writing;
    let change =
        move |change: DeviceAccessChange| on_access.call(AccessCommand::Change { device, change });
    let label = use_signal(String::new);
    let password = use_signal(String::new);
    let tier = use_signal(|| AccessTier::Play);
    let open = panel.open;
    rsx! {
        div { class: "tw:grid tw:gap-1",
            p { class: "tw:m-0 tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                "Passwords set from this browser"
            }
            if panel.secrets.is_empty() {
                p { class: HELP_CLASS, "None yet." }
            }
            ul { class: "tw:m-0 tw:grid tw:list-none tw:p-0",
                for secret in panel.secrets.iter().cloned() {
                    SecretRow {
                        key: "{secret.label}",
                        label: secret.label.clone(),
                        tier: secret.tier,
                        disabled: busy,
                        on_remove: move |_| change(DeviceAccessChange::Revoke { label: secret.label.clone() }),
                    }
                }
            }
            // Add sits where the new row will appear: under the list.
            form {
                class: "tw:grid tw:gap-1.5 tw:pt-1",
                onsubmit: move |event| {
                    event.prevent_default();
                    let mut label = label;
                    let mut password = password;
                    change(DeviceAccessChange::Add(NewSecret {
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
                PasswordField { value: password, initially_shown: show_passwords }
                button { class: quiet_action_class(), r#type: "submit", disabled: busy, "Add password" }
            }
            p { class: HELP_CLASS,
                "The piece may also hold passwords added from another browser. Saving from here replaces them."
            }
            button {
                class: TEXT_BUTTON_CLASS,
                r#type: "button",
                disabled: busy,
                title: "Write this list to the piece, replacing anything else it holds",
                onclick: move |_| change(DeviceAccessChange::ReplaceAll),
                "Replace all with this list"
            }
        }
        label { class: "tw:flex tw:items-start tw:gap-2 tw:text-sm tw:text-strong-foreground",
            input {
                r#type: "checkbox",
                checked: open,
                disabled: busy,
                onchange: move |event| change(DeviceAccessChange::SetOpen(event.checked())),
            }
            span { class: "tw:grid tw:gap-0.5",
                span { "Open, no password (play only)" }
                span { class: HELP_CLASS, "Anyone nearby can turn its knobs. Editing still needs a password." }
            }
        }
        button {
            class: outline_action_class(true),
            r#type: "button",
            disabled: busy,
            onclick: move |_| change(DeviceAccessChange::Disable),
            "Turn Bluetooth off"
        }
    }
}

/// Bluetooth is off (or unknown here): turn it on, locked with a password
/// — the account default pre-filled — or open, as an explicit choice.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TurnOnForm(
    default_password: Option<String>,
    busy: bool,
    show_passwords: bool,
    on_change: EventHandler<DeviceAccessChange>,
) -> Element {
    let password = use_signal(|| default_password.clone().unwrap_or_default());
    let label = use_signal(|| "default".to_string());
    let tier = use_signal(|| AccessTier::Edit);
    rsx! {
        form {
            class: "tw:grid tw:gap-2",
            onsubmit: move |event| {
                event.prevent_default();
                on_change.call(DeviceAccessChange::Enable {
                    secret: Some(NewSecret {
                        label: label.read().trim().to_string(),
                        tier: tier(),
                        password: password.read().clone(),
                    }),
                    open: false,
                });
            },
            p { class: "tw:m-0 tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                "Turn on, locked"
            }
            PasswordField { value: password, initially_shown: show_passwords }
            if default_password.is_some() {
                p { class: HELP_CLASS, "Your default device password is filled in. Change it for this piece if you like." }
            }
            p { class: HELP_CLASS, "Its name, and what it can do:" }
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                NameField { value: label, label: "Name".to_string() }
                TierChoice { tier }
            }
            button { class: outline_action_class(false), r#type: "submit", disabled: busy, "Turn on Bluetooth" }
        }
        div { class: "tw:grid tw:gap-1 tw:border-t tw:border-border tw:pt-2",
            button {
                class: quiet_action_class(),
                r#type: "button",
                disabled: busy,
                onclick: move |_| on_change.call(DeviceAccessChange::Enable { secret: None, open: true }),
                "Turn on open, no password (play only)"
            }
            p { class: HELP_CLASS, "Anyone nearby can turn its knobs. Editing still needs a password." }
        }
    }
}

/// The panel's first sentence: where Bluetooth stands, as far as this
/// browser knows.
pub(crate) fn state_sentence(panel: &UiAccessPanel) -> &'static str {
    match (panel.ble_enabled, panel.open, panel.secrets.is_empty()) {
        (None, _, _) => {
            "Not set from this browser. Bluetooth is off unless someone turned it on elsewhere."
        }
        (Some(false), _, _) => "Off. This piece is reached by USB only.",
        (Some(true), true, _) => "On and open: anyone nearby can play. Editing needs a password.",
        (Some(true), false, true) => "On, but no password is set here — add one.",
        (Some(true), false, false) => "On and locked: only these passwords reach it.",
    }
}

/// What the restart note says. The board reads the Bluetooth switch once,
/// at boot (M4), so a change waits for a restart.
fn restart_sentence(panel: &UiAccessPanel) -> String {
    let now = match panel.ble_enabled {
        Some(true) => "on",
        _ => "off",
    };
    match panel.can_restart {
        true => format!("Bluetooth turns {now} when the piece restarts."),
        false => format!(
            "Bluetooth turns {now} when the piece restarts — connect it by USB to restart it from here, or power it off and on."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(ble_enabled: Option<bool>, open: bool, secrets: usize) -> UiAccessPanel {
        UiAccessPanel {
            device: lpa_studio_core::DeviceId(1),
            ble_enabled,
            open,
            secrets: (0..secrets)
                .map(|n| lpa_studio_core::UiAccessSecret {
                    label: format!("pw{n}"),
                    tier: AccessTier::Play,
                })
                .collect(),
            restart_pending: false,
            can_restart: true,
            writing: false,
            error: None,
            default_password: None,
        }
    }

    #[test]
    fn every_state_says_where_bluetooth_stands_in_plain_words() {
        assert!(state_sentence(&panel(None, false, 0)).contains("Not set from this browser"));
        assert!(state_sentence(&panel(Some(false), false, 0)).contains("USB only"));
        assert!(state_sentence(&panel(Some(true), true, 0)).contains("anyone nearby can play"));
        assert!(state_sentence(&panel(Some(true), false, 2)).contains("locked"));
    }

    #[test]
    fn the_restart_note_says_how_over_bluetooth_too() {
        let mut over_ble = panel(Some(true), false, 1);
        over_ble.can_restart = false;
        assert!(restart_sentence(&over_ble).contains("power it off and on"));
        assert!(restart_sentence(&panel(Some(true), false, 1)).contains("turns on"));
    }
}
