//! "Who has access" (spike §2): every key on the device, read from it.
//!
//! It opens from the card's Connections group ("Who has access · N ›") into
//! a panel in the top layer, so the card keeps its height. Only a link that
//! holds edit sees it: USB (the trusted link) or a Bluetooth unlock at edit
//! — the board answers the list at edit only.
//!
//! One flat list, in the order people think of them: this browser, other
//! browsers, accounts, the account's passwords, shared passwords
//! ([`super::access_entry_row::ordered`]), then the "Anyone nearby" switch.
//! Each row's trash can is the two-tap confirm. "+ Add a password" at the
//! list's end swaps the panel for Share ([`super::share_access_sheet`]) in
//! place — the add sits where the new row will appear.

use dioxus::prelude::*;
use lpa_studio_core::{AccessCommand, DeviceAccessChange, UiAccessPanel};

use super::access_entry_row::{AccessEntryRow, ICON_TILE_CLASS, ordered};
use super::access_fields::{HELP_CLASS, Switch, TEXT_LINK_CLASS};
use super::share_access_sheet::ShareAccessSheet;
use crate::base::{StudioIcon, StudioIconName};

/// The panel body (the popover's content, and the stories' subject).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceAccessPanel(
    panel: UiAccessPanel,
    /// The device's name, for Share's title and link.
    device_name: String,
    on_access: EventHandler<AccessCommand>,
    /// Stories only: the entry (by salt) whose trash can starts armed.
    #[props(default)]
    armed_preview: Option<[u8; 16]>,
    /// Stories only: open on Share.
    #[props(default)]
    sharing_preview: bool,
    /// Stories only: Share's fixed words.
    #[props(default)]
    share_words: Option<String>,
    /// Stories only: the product's origin for Share's link.
    #[props(default)]
    share_origin: Option<String>,
) -> Element {
    let device = panel.device;
    let busy = panel.writing;
    let mut sharing = use_signal(|| sharing_preview);
    if sharing() {
        return rsx! {
            ShareAccessSheet {
                device,
                device_name,
                on_access,
                on_done: move |_| sharing.set(false),
                words: share_words.clone(),
                origin: share_origin.clone(),
                busy,
            }
        };
    }
    let open = panel.open;
    let entries = ordered(&panel.entries);
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:py-1.5",
            div { class: "tw:flex tw:items-center tw:gap-2 tw:text-status-neutral-foreground",
                StudioIcon { name: StudioIconName::AccessPeople, size: 15 }
                h3 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "Who has access" }
            }
            if panel.ble_enabled.is_none() {
                p { class: HELP_CLASS, "Reading the device's list…" }
            }
            ul { class: "tw:m-0 tw:grid tw:list-none tw:p-0",
                for entry in entries {
                    AccessEntryRow {
                        key: "{entry.salt_id:?}",
                        armed_preview: armed_preview == Some(entry.salt_id),
                        entry,
                        device,
                        busy,
                        on_access,
                    }
                }
                li { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:py-2 tw:first:border-t-0",
                    span { class: "{ICON_TILE_CLASS} tw:border-status-live-border tw:bg-status-live-bg tw:text-status-live-foreground",
                        StudioIcon { name: StudioIconName::AccessNearby, size: 15 }
                    }
                    span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                        span { class: "tw:text-[13px] tw:font-bold tw:text-strong-foreground", "Anyone nearby" }
                        span { class: "tw:truncate tw:text-[11px] tw:text-dim-foreground",
                            if open { "can play without a password" } else { "off — needs a key or password" }
                        }
                    }
                    Switch {
                        on: open,
                        label: "Anyone nearby can play".to_string(),
                        locked: busy || panel.ble_enabled.is_none(),
                        on_toggle: move |on| on_access.call(AccessCommand::Change {
                            device,
                            change: DeviceAccessChange::SetOpen(on),
                        }),
                    }
                }
            }
            // Add sits where the new row will appear: under the list.
            div { class: "tw:flex tw:flex-wrap tw:items-center tw:justify-between tw:gap-x-3 tw:gap-y-1",
                button {
                    class: TEXT_LINK_CLASS,
                    r#type: "button",
                    disabled: busy || panel.ble_enabled.is_none(),
                    onclick: move |_| sharing.set(true),
                    "+ Add a password"
                }
                span { class: HELP_CLASS, "USB always gets in." }
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
