//! The device access panel: who has access to this device, read from it.
//!
//! It opens from the device card's own verb row into a panel in the top
//! layer, so the card keeps its fixed height. It is offered over USB — the
//! trusted link — and over a Bluetooth unlock at edit.
//!
//! The list is the device's own (`AccessList`): every browser, account and
//! password on it, from any browser. Changes go to the device one at a time
//! (add, remove, a switch), and the device merges them.
//!
//! This is the minimal panel over P3's core; the spike's Connections group
//! and "Who has access" sheet replace it in P4.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, AccessTier, DeviceAccessChange, UiAccessPanel};

use super::access_fields::{HELP_CLASS, NameField, PasswordField, SecretRow, TierChoice};
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
    let label = use_signal(String::new);
    let password = use_signal(String::new);
    let tier = use_signal(|| AccessTier::Play);
    let on = panel.ble_enabled == Some(true);
    let open = panel.open;
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:py-1.5",
            h3 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
                "Who has access · {panel.count}"
            }
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

            ul { class: "tw:m-0 tw:grid tw:list-none tw:p-0",
                for entry in panel.entries.iter().cloned() {
                    SecretRow {
                        key: "{entry.label}-{entry.salt_id:?}",
                        label: if entry.is_this_browser { format!("{} (this browser)", entry.label) } else { entry.label.clone() },
                        tier: entry.tier,
                        disabled: busy,
                        on_remove: move |_| change(DeviceAccessChange::Remove { salt: entry.salt_id }),
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
                    change(DeviceAccessChange::AddPassword {
                        label: label.read().trim().to_string(),
                        tier: tier(),
                        password: password.read().clone(),
                    });
                    label.set(String::new());
                    password.set(String::new());
                },
                div { class: "tw:flex tw:min-w-0 tw:gap-1.5",
                    NameField { value: label, label: "Name (friends, crew…)".to_string() }
                    TierChoice { tier }
                }
                PasswordField { value: password, initially_shown: show_passwords }
                button { class: quiet_action_class(), r#type: "submit", disabled: busy, "Add device password" }
            }
            label { class: "tw:flex tw:items-start tw:gap-2 tw:text-sm tw:text-strong-foreground",
                input {
                    r#type: "checkbox",
                    checked: open,
                    disabled: busy,
                    onchange: move |event| change(DeviceAccessChange::SetOpen(event.checked())),
                }
                span { class: "tw:grid tw:gap-0.5",
                    span { "Anyone nearby can play" }
                    span { class: HELP_CLASS, "Editing still needs a key or a device password." }
                }
            }
            button {
                class: outline_action_class(on),
                r#type: "button",
                disabled: busy || (on && panel.over_bluetooth),
                title: if on && panel.over_bluetooth { "Turn off by USB" } else { "" },
                onclick: move |_| change(DeviceAccessChange::SetBluetooth(!on)),
                if on { "Turn Bluetooth off" } else { "Turn Bluetooth on" }
            }

            if busy {
                p { class: HELP_CLASS, "Writing to the device…" }
            }
            if let Some(error) = panel.error.clone() {
                p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground", "{error}" }
            }
        }
    }
}

/// The panel's first sentence: where Bluetooth stands on the device.
pub(crate) fn state_sentence(panel: &UiAccessPanel) -> &'static str {
    match (panel.ble_enabled, panel.open) {
        (None, _) => "Reading the device's list…",
        (Some(false), _) => "Bluetooth is off. This device is reached by USB only.",
        (Some(true), true) => "Bluetooth is on, and anyone nearby can play.",
        (Some(true), false) => "Bluetooth is on. Only these can unlock it.",
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
        true => format!("Bluetooth turns {now} when the device restarts."),
        false => format!(
            "Bluetooth turns {now} when the device restarts — connect it by USB to restart it from here, or power it off and on."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_says_where_bluetooth_stands_in_plain_words() {
        assert!(state_sentence(&panel(None, false)).contains("Reading"));
        assert!(state_sentence(&panel(Some(false), false)).contains("USB only"));
        assert!(state_sentence(&panel(Some(true), true)).contains("anyone nearby can play"));
        assert!(state_sentence(&panel(Some(true), false)).contains("Only these"));
    }

    #[test]
    fn the_restart_note_says_how_over_bluetooth_too() {
        let mut over_ble = panel(Some(true), false);
        over_ble.can_restart = false;
        assert!(restart_sentence(&over_ble).contains("power it off and on"));
        assert!(restart_sentence(&panel(Some(true), false)).contains("turns on"));
    }

    fn panel(ble_enabled: Option<bool>, open: bool) -> UiAccessPanel {
        UiAccessPanel {
            device: lpa_studio_core::DeviceId(1),
            entries: Vec::new(),
            count: usize::from(open),
            ble_enabled,
            open,
            restart_pending: false,
            can_restart: true,
            over_bluetooth: false,
            writing: false,
            error: None,
        }
    }
}
