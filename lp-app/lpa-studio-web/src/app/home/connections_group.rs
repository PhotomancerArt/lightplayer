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
    let bluetooth = lpa_studio_core::bluetooth_switch(&access);
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
