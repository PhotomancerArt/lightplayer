//! The device card's Connections group (spike §1): how Studio reaches the
//! device, one row per link.
//!
//! | row | what it shows |
//! |---|---|
//! | USB | "connected" / "not connected" |
//! | Bluetooth | the icon, "Bluetooth", a switch — nothing else |
//! | Who has access · N › | opens the list (edit links only) |
//!
//! Bluetooth is on by default, so most people never touch the switch. The
//! board reads it once, at boot: flipped over USB, Studio restarts the
//! device to apply it and the row says so until the device is back. Over
//! Bluetooth the switch is locked — you cannot turn off the radio you are
//! talking over — and the row says how instead. (Wi‑Fi joins as a row
//! later.)

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, DeviceAccessChange, DeviceId, UiDeviceAccess};

use super::access_fields::Switch;
use super::device_access_panel::DeviceAccessPanel;
use crate::base::{DetailPopover, DetailSection, PopoverPlacement, StudioIcon, StudioIconName};

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectionsGroup(
    device: DeviceId,
    device_name: String,
    access: UiDeviceAccess,
    on_access: EventHandler<AccessCommand>,
    /// Stories only: mount "Who has access" open.
    #[props(default)]
    who_open: bool,
) -> Element {
    let panel = access.panel.clone();
    let over_bluetooth = access.over_bluetooth;
    let bluetooth = bluetooth_row(&access);
    rsx! {
        div { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card-subtle",
            div { class: ROW_CLASS,
                span { class: "tw:inline-flex tw:flex-none tw:text-status-neutral-foreground",
                    StudioIcon { name: StudioIconName::Usb, size: 15 }
                }
                span { class: "tw:min-w-0 tw:flex-1", "USB" }
                span { class: VALUE_CLASS,
                    if over_bluetooth { "not connected" } else { "connected" }
                }
            }
            div { class: ROW_CLASS,
                span { class: "tw:inline-flex tw:flex-none tw:text-status-live-foreground",
                    StudioIcon { name: StudioIconName::Bluetooth, size: 15 }
                }
                span { class: "tw:grid tw:min-w-0 tw:flex-1",
                    "Bluetooth"
                    if let Some(sub) = bluetooth.sub {
                        span { class: "tw:text-[11px] tw:font-medium tw:text-dim-foreground", "{sub}" }
                    }
                }
                Switch {
                    on: bluetooth.on,
                    label: "Bluetooth".to_string(),
                    locked: bluetooth.locked,
                    on_toggle: move |on| on_access.call(AccessCommand::Change {
                        device,
                        change: DeviceAccessChange::SetBluetooth(on),
                    }),
                }
            }
            if let Some(note) = bluetooth.restart_note {
                p { class: "tw:m-0 tw:-mt-1 tw:pb-2.5 tw:pl-[37px] tw:pr-3 tw:text-[11.5px] tw:leading-snug tw:text-status-warning-foreground",
                    "{note}"
                }
            }
            if let Some(panel) = panel {
                // The popover's own wrapper is an inline grid that centres
                // its trigger; the row must span the group.
                div { class: "tw:grid tw:[&>span]:w-full tw:[&>span]:place-items-stretch",
                DetailPopover {
                    icon: StudioIconName::AccessPeople,
                    label: "Who has access".to_string(),
                    title: "Who has access".to_string(),
                    placement: PopoverPlacement::TopEnd,
                    initially_open: who_open,
                    layer_keeps_layout: true,
                    trigger_class: WHO_ROW_CLASS.to_string(),
                    trigger_open_class: format!("{WHO_ROW_CLASS} tw:bg-white/5"),
                    trigger: rsx! {
                        span { class: "tw:inline-flex tw:flex-none tw:text-status-neutral-foreground",
                            StudioIcon { name: StudioIconName::AccessPeople, size: 15 }
                        }
                        span { class: "tw:min-w-0 tw:flex-1 tw:text-left", "Who has access" }
                        span { class: VALUE_CLASS,
                            "{panel.count}"
                            StudioIcon { name: StudioIconName::Collapsed, size: 14 }
                        }
                    },
                    DetailSection {
                        DeviceAccessPanel { panel, device_name, on_access }
                    }
                }
                }
            }
        }
    }
}

/// What the Bluetooth row shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BluetoothRow {
    pub on: bool,
    pub locked: bool,
    /// Under the name: why it is locked.
    pub sub: Option<&'static str>,
    /// Under the row: the restart that applies a change.
    pub restart_note: Option<String>,
}

/// Decided apart from the component, so it is testable.
pub(crate) fn bluetooth_row(access: &UiDeviceAccess) -> BluetoothRow {
    if access.over_bluetooth {
        return BluetoothRow {
            on: true,
            locked: true,
            sub: Some("connected this way — turn off by USB"),
            restart_note: None,
        };
    }
    let Some(panel) = access.panel.as_ref() else {
        return BluetoothRow {
            on: false,
            locked: true,
            sub: None,
            restart_note: None,
        };
    };
    let on = panel.ble_enabled.unwrap_or(false);
    let word = if on { "on" } else { "off" };
    let restart_note = panel.restart_pending.then(|| match panel.can_restart {
        true => format!("Restarting to turn Bluetooth {word}…"),
        false => format!("Bluetooth turns {word} when the device restarts."),
    });
    BluetoothRow {
        on,
        locked: panel.ble_enabled.is_none() || panel.writing || panel.restart_pending,
        sub: None,
        restart_note,
    }
}

const ROW_CLASS: &str = "tw:flex tw:min-h-11 tw:min-w-0 tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:px-3 tw:py-2 tw:text-[13.5px] tw:font-semibold tw:text-strong-foreground tw:first:border-t-0";

/// The Who row is the popover's trigger: a full-width button that reads as
/// a row.
const WHO_ROW_CLASS: &str = "tw:flex tw:min-h-11 tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2.5 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:bg-transparent tw:px-3 tw:py-2 tw:text-[13.5px] tw:font-semibold tw:text-strong-foreground tw:hover:bg-white/5 ux-focus-ring";

const VALUE_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:gap-1.5 tw:text-xs tw:font-semibold tw:text-subtle-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::UiAccessPanel;

    #[test]
    fn over_bluetooth_the_switch_is_locked_on_and_says_how() {
        let row = bluetooth_row(&UiDeviceAccess {
            over_bluetooth: true,
            ..UiDeviceAccess::default()
        });
        assert!(row.on && row.locked);
        assert_eq!(row.sub, Some("connected this way — turn off by USB"));
    }

    #[test]
    fn a_switch_over_usb_says_the_restart_until_the_device_is_back() {
        let mut access = usb(Some(true));
        assert_eq!(bluetooth_row(&access).restart_note, None);
        assert!(!bluetooth_row(&access).locked);
        access.panel.as_mut().unwrap().restart_pending = true;
        let row = bluetooth_row(&access);
        assert_eq!(
            row.restart_note.as_deref(),
            Some("Restarting to turn Bluetooth on…")
        );
        assert!(row.locked, "no second flip while the first applies");
    }

    #[test]
    fn before_the_list_arrives_the_switch_waits() {
        assert!(bluetooth_row(&usb(None)).locked);
    }

    fn usb(ble_enabled: Option<bool>) -> UiDeviceAccess {
        UiDeviceAccess {
            over_bluetooth: false,
            line: None,
            unlock: None,
            panel: Some(UiAccessPanel {
                device: DeviceId(1),
                entries: Vec::new(),
                count: 0,
                ble_enabled,
                open: false,
                restart_pending: false,
                can_restart: true,
                over_bluetooth: false,
                writing: false,
                error: None,
            }),
        }
    }
}
