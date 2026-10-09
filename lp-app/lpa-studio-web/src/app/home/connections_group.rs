//! The device card's Connections group (spike §1): how Studio reaches the
//! device, one row per link.
//!
//! | row | what it shows |
//! |---|---|
//! | USB | "connected" / "not connected" |
//! | Bluetooth | the icon, "Bluetooth", a switch — nothing else |
//! | Access · open › | opens the access panel (author links only) |
//! | Wi‑Fi · <network> › | opens the Wi‑Fi popover ([`super::wifi_panel`]): the board's saved networks, set over this link |
//!
//! Bluetooth is on by default, so most people never touch the switch. The
//! board reads it once, at boot: flipped over USB, Studio restarts the
//! device to apply it and the row says so until the device is back. Over
//! Bluetooth the switch is locked — you cannot turn off the radio you are
//! talking over — and the row says how instead.
//!
//! The Wi‑Fi row shows on every LightPlayer board a link reaches (core's
//! [`UiDeviceWifi`]); its verbs are offers at `devices/<board>/wifi/…`,
//! published only while the link holds author — below that the popover
//! says what it needs. Opening it asks the board again (and what it hears,
//! on a board that can scan).

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessCommand, DeviceAccessChange, DeviceId, NetworkCommand, OpenTo, UiAction, UiDeviceAccess,
    UiDeviceWifi, UiOffer, WifiTone, open_summary,
};

use super::access_fields::Switch;
use super::device_access_panel::DeviceAccessPanel;
use super::wifi_panel::{SignalBars, WifiPanel};
use crate::base::{DetailPopover, PopoverPlacement, StudioIcon, StudioIconName};
use crate::core::offer::use_offers;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectionsGroup(
    device: DeviceId,
    access: UiDeviceAccess,
    on_access: EventHandler<AccessCommand>,
    /// Stories only: mount the access panel open.
    #[props(default)]
    who_open: bool,
    /// Stories only: mount the access panel's keys list open too.
    #[props(default)]
    keys_open_preview: bool,
    /// The board's Wi‑Fi facts; `None` shows no Wi‑Fi row.
    #[props(default)]
    wifi: Option<UiDeviceWifi>,
    /// Where the Wi‑Fi panel's presses go.
    on_action: EventHandler<UiAction>,
    /// The Wi‑Fi panel's refresh on open.
    on_network: EventHandler<NetworkCommand>,
    /// Stories only: mount the Wi‑Fi panel open.
    #[props(default)]
    wifi_open: bool,
    /// Stories only: the Wi‑Fi verbs (the app reads them from the offer
    /// tree, which a story has none of).
    #[props(default)]
    wifi_offers_preview: Option<Vec<UiOffer>>,
    /// Stories only: what the Wi‑Fi form starts holding.
    #[props(default)]
    wifi_args_preview: Option<lpa_studio_core::OfferArgs>,
    /// Stories only: the Wi‑Fi panel's Forget starts armed.
    #[props(default)]
    wifi_forget_armed_preview: bool,
) -> Element {
    let panel = access.panel.clone();
    // The Wi‑Fi verbs: `devices/<board>/wifi/…` in the view's tree.
    let tree = use_offers();
    let wifi_offers = wifi_offers_preview.unwrap_or_else(|| {
        let tree = tree.read();
        tree.device_prefix(device)
            .map(|prefix| {
                tree.own_verbs_of(&prefix.clone().child("wifi"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    });
    // Opening the Wi‑Fi popover asks the board again (plan Q7), and what it
    // hears (core asks nothing of a board that cannot scan).
    let wifi_open_signal = use_signal(|| wifi_open);
    use_effect(move || {
        if wifi_open_signal() {
            on_network.call(NetworkCommand::Refresh { device });
            on_network.call(NetworkCommand::Scan { device });
        }
    });
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
                    label: "Access".to_string(),
                    title: "Access".to_string(),
                    placement: PopoverPlacement::TopEnd,
                    initially_open: who_open,
                    layer_keeps_layout: true,
                    trigger_class: WHO_ROW_CLASS.to_string(),
                    trigger_open_class: format!("{WHO_ROW_CLASS} tw:bg-white/5"),
                    trigger: rsx! {
                        span { class: "tw:inline-flex tw:flex-none tw:text-status-neutral-foreground",
                            StudioIcon { name: StudioIconName::AccessPeople, size: 15 }
                        }
                        span { class: "tw:min-w-0 tw:flex-1 tw:text-left", "Access" }
                        // A new board is open to anyone nearby: the card
                        // says so where it can be seen.
                        span { class: if panel.open == OpenTo::Edit { OPEN_VALUE_CLASS } else { VALUE_CLASS },
                            if panel.ble_enabled.is_some() { "{open_summary(panel.open)}" }
                            StudioIcon { name: StudioIconName::Collapsed, size: 14 }
                        }
                    },
                    DeviceAccessPanel { panel, on_access, keys_open_preview }
                }
                }
            }
            if let Some(wifi) = wifi {
                div { class: "tw:grid tw:[&>span]:w-full tw:[&>span]:place-items-stretch",
                DetailPopover {
                    icon: StudioIconName::Wifi,
                    label: "Wi‑Fi".to_string(),
                    title: "Wi‑Fi".to_string(),
                    placement: PopoverPlacement::TopEnd,
                    open_signal: Some(wifi_open_signal),
                    layer_keeps_layout: true,
                    trigger_class: WHO_ROW_CLASS.to_string(),
                    trigger_open_class: format!("{WHO_ROW_CLASS} tw:bg-white/5"),
                    trigger: rsx! {
                        span { class: "tw:inline-flex tw:flex-none tw:text-status-neutral-foreground",
                            StudioIcon { name: StudioIconName::Wifi, size: 15 }
                        }
                        span { class: "tw:min-w-0 tw:flex-1 tw:text-left", "Wi‑Fi" }
                        span { class: wifi_value_class(wifi.row_tone()),
                            if let Some(rssi) = wifi.row_rssi() {
                                SignalBars { rssi: Some(rssi), in_use: true }
                            }
                            span { class: "tw:max-w-36 tw:truncate", "{wifi.row_value()}" }
                            StudioIcon { name: StudioIconName::Collapsed, size: 14 }
                        }
                    },
                    WifiPanel {
                        wifi,
                        offers: wifi_offers,
                        on_action,
                        on_network,
                        args_preview: wifi_args_preview,
                        forget_armed_preview: wifi_forget_armed_preview,
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

/// The Wi‑Fi row's value, in its tone: plain, the strong text of a
/// connected network, or amber for a refused password.
fn wifi_value_class(tone: WifiTone) -> &'static str {
    match tone {
        WifiTone::Plain => VALUE_CLASS,
        WifiTone::Good => {
            "tw:inline-flex tw:flex-none tw:items-center tw:gap-1.5 tw:text-xs tw:font-semibold tw:text-strong-foreground"
        }
        WifiTone::Warn => OPEN_VALUE_CLASS,
    }
}

const VALUE_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:gap-1.5 tw:text-xs tw:font-semibold tw:text-subtle-foreground";

/// The access row's value while anyone nearby can author: warning-tinted.
const OPEN_VALUE_CLASS: &str = "tw:inline-flex tw:flex-none tw:items-center tw:gap-1.5 tw:text-xs tw:font-semibold tw:text-status-warning-foreground";

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
                ble_enabled,
                ..UiAccessPanel::reading(DeviceId(1))
            }),
            account_key_refused: None,
        }
    }
}
