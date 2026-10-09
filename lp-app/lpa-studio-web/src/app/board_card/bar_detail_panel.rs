//! [`BarDetailPanel`]: one of today's card surfaces inside a details card
//! ([`UiDetailPanel`]), drawn as a section (or several) of that card —
//! never a framed box of its own, never a popover inside the popover.
//!
//! | panel | drawn with | in |
//! |---|---|---|
//! | Terminal | [`DeviceTerminal`], flush | the status corner's details |
//! | Access | [`DeviceAccessPanel`] (its own sections) | the access bar's |
//! | Bluetooth | [`BluetoothSwitchPanel`] | the connection bar's |
//! | Wi‑Fi | [`WifiPanel`], in-card (its verbs at `<board>/wifi/…`) | the connection bar's |
//! | Link counters | [`LinkCountersSection`] | the connection bar's |
//! | Rename | [`DeviceRenameSection`] over the `rename` offer | the hardware bar's |
//! | Layout | [`DeviceLayoutSheet`]: the question or the refusal | the firmware bar's (raised) |
//! | Other version | [`OtherVersionForm`], inline | the firmware bar's |
//! | Restore from file | [`RestoreFromFileButton`] | the firmware bar's |
//!
//! The panels' words are core's payload or the moved panels' own,
//! unchanged; their verbs are offers drawn from the tree. The access
//! panel's edits, the Bluetooth switch and the Wi‑Fi refresh still send
//! their access and network commands, as today (out of scope here).

use dioxus::prelude::*;
use lpa_studio_core::{DeviceId, OfferPath, UiAction, UiDetailPanel, UiDeviceWifi, UiLayoutPanel};

use super::bluetooth_switch_panel::BluetoothSwitchPanel;
use super::card_action::use_card_scope;
use super::link_counters_section::LinkCountersSection;
use super::other_version_form::OtherVersionForm;
use super::restore_from_file_button::RestoreFromFileButton;
use crate::app::agent::AgentMark;
use crate::app::home::access_ui_context::{access_handler, network_handler};
use crate::app::home::device_access_panel::DeviceAccessPanel;
use crate::app::home::device_layout_sheet::DeviceLayoutSheet;
use crate::app::home::device_rename_section::DeviceRenameSection;
use crate::app::home::device_terminal::DeviceTerminal;
use crate::app::home::wifi_panel::WifiPanel;
use crate::base::DetailSection;
use crate::core::{use_offer_at, use_offers};

/// One panel. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BarDetailPanel(
    panel: UiDetailPanel,
    /// `devices/<board ref>`: the board the panel acts on.
    board: OfferPath,
    /// A refusal's Close: the details remember that refusal closed.
    #[props(default)]
    on_close_layout: Option<EventHandler<UiLayoutPanel>>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let scope = use_card_scope();
    let on_access = access_handler();
    let device = scope.device.unwrap_or(DeviceId(0));
    // Every variant drawn, no wildcard: a new panel fails to compile until
    // it has a home here.
    match panel {
        UiDetailPanel::Terminal { lines, dropped } => rsx! {
            DeviceTerminal { lines, dropped, height_class: TERMINAL_HEIGHT_CLASS }
        },
        UiDetailPanel::Access(panel) => rsx! {
            DeviceAccessPanel {
                panel,
                on_access,
                keys_open_preview: scope.previews.access_keys_open,
            }
        },
        UiDetailPanel::Bluetooth(switch) => rsx! {
            BluetoothSwitchPanel { switch, device, on_access }
        },
        UiDetailPanel::Wifi(wifi) => rsx! {
            WifiDetails { wifi, board, on_action }
        },
        UiDetailPanel::Rename { offer, title } => rsx! {
            RenameDetails { offer, title, on_action }
        },
        UiDetailPanel::LinkCounters(counters) => rsx! {
            LinkCountersSection { counters }
        },
        UiDetailPanel::Layout(panel) => {
            // A refusal (no Cancel: nothing is running) closes in the page.
            let on_close = match (panel.cancel.is_none(), on_close_layout) {
                (true, Some(close)) => {
                    let refusal = panel.clone();
                    Some(EventHandler::new(move |_| close.call(refusal.clone())))
                }
                _ => None,
            };
            rsx! {
                DeviceLayoutSheet { panel, on_close, on_action }
            }
        }
        UiDetailPanel::OtherVersion { install, from_file } => rsx! {
            OtherVersionDetails {
                install,
                from_file,
                device,
                preview: scope.previews.other_version.clone(),
                on_action,
            }
        },
        UiDetailPanel::RestoreFromFile {
            offer,
            current_base_mac,
        } => rsx! {
            RestoreDetails { offer, current_base_mac, device, on_action }
        },
    }
}

/// The Wi‑Fi panel in the card, with the board's Wi‑Fi verbs from the tree
/// (`<board>/wifi/…`).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WifiDetails(wifi: UiDeviceWifi, board: OfferPath, on_action: EventHandler<UiAction>) -> Element {
    let tree = use_offers();
    let offers = tree
        .read()
        .own_verbs_of(&board.child("wifi"))
        .cloned()
        .collect::<Vec<_>>();
    let on_network = network_handler();
    rsx! {
        WifiPanel { wifi, offers, on_action, on_network, in_card: true }
    }
}

/// Rename, over the `rename` offer the tree holds; nothing when it is gone.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RenameDetails(offer: OfferPath, title: String, on_action: EventHandler<UiAction>) -> Element {
    let Some(rename) = use_offer_at(offer)() else {
        return rsx! {};
    };
    // The section marks its own form with the offer's path.
    rsx! {
        DeviceRenameSection { offer: rename, title, on_action }
    }
}

/// "Other version…", inline, over the install offer (and From a file…'s)
/// the tree holds: a section of the firmware details titled in the
/// offer's own words.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OtherVersionDetails(
    install: OfferPath,
    from_file: Option<OfferPath>,
    device: DeviceId,
    preview: Option<super::other_version_form::OfferPickerPreview>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let tree = use_offers();
    let (install, from_file) = {
        let tree = tree.read();
        (
            tree.get(&install).cloned(),
            from_file.and_then(|path| tree.get(&path).cloned()),
        )
    };
    let Some(install) = install else {
        return rsx! {};
    };
    rsx! {
        DetailSection { title: Some(install.label().to_string()),
            div { class: "tw:py-1",
                OtherVersionForm { device, install, from_file, preview, on_action }
            }
        }
    }
}

/// "Restore from a backup file…", while the tree offers it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RestoreDetails(
    offer: OfferPath,
    current_base_mac: Option<String>,
    device: DeviceId,
    on_action: EventHandler<UiAction>,
) -> Element {
    if use_offer_at(offer.clone())().is_none() {
        return rsx! {};
    }
    rsx! {
        section { class: RESTORE_SECTION_CLASS,
            AgentMark { path: offer,
                RestoreFromFileButton { device, current_base_mac, on_action }
            }
        }
    }
}

/// The terminal's fixed height inside a details card: a long log scrolls,
/// never grows the card.
pub(crate) const TERMINAL_HEIGHT_CLASS: &str = "tw:h-40";

/// The restore row's section: the divider and padding, no frame.
const RESTORE_SECTION_CLASS: &str = "tw:grid tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:px-1.5 tw:py-1 tw:first:border-t-0";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        DeviceLinkCounters, UiAccessPanel, UiBluetoothSwitch, UiOffer, UiOfferTree,
    };

    use super::*;
    use crate::app::board_card::card_test_fixtures::{
        attribute_values, board, card_and_tree, porch_view, render,
    };
    use crate::core::OffersProvider;

    /// Every panel draws a piece inside a details card — what each one
    /// shows, and the offer path on each that presses one. (The match in
    /// [`BarDetailPanel`] has no wildcard: a new panel does not compile
    /// until it is drawn, and [`marker`] below has none either.)
    #[test]
    fn every_panel_draws_its_piece() {
        let tree = tree_with_firmware_verbs();
        for panel in every_panel() {
            let (marker, path) = marker(&panel);
            let html = render_panel(tree.clone(), panel);
            assert!(html.contains(marker), "`{marker}` missing: {html}");
            if let Some(path) = path {
                let marked = attribute_values(&html, "data-offer-path");
                assert!(
                    marked.contains(&path.to_string()),
                    "{path} unmarked: {marked:?}"
                );
            }
        }
    }

    /// A panel whose offer the tree no longer holds draws nothing to press.
    #[test]
    fn a_panel_whose_offer_is_gone_draws_nothing_to_press() {
        let gone = UiDetailPanel::RestoreFromFile {
            offer: board().child("restore-from-file"),
            current_base_mac: None,
        };
        let html = render_panel(UiOfferTree::new(), gone);
        assert!(!html.contains("<button"), "{html}");
    }

    /// One of each panel.
    fn every_panel() -> Vec<UiDetailPanel> {
        vec![
            UiDetailPanel::Terminal {
                lines: Vec::new(),
                dropped: 3,
            },
            UiDetailPanel::Access(UiAccessPanel::reading(DeviceId(7))),
            UiDetailPanel::Bluetooth(UiBluetoothSwitch {
                on: true,
                locked: false,
                sub: None,
                restart_note: Some("Restarting to turn Bluetooth on…".to_string()),
            }),
            UiDetailPanel::Wifi(UiDeviceWifi::new(DeviceId(7), true)),
            UiDetailPanel::Rename {
                offer: board().child("rename"),
                title: "Porch".to_string(),
            },
            UiDetailPanel::LinkCounters(DeviceLinkCounters {
                resends: 3,
                damaged: 0,
                resets: 1,
                stalls: 0,
                bytes_sent: 1_000,
                bytes_received: 2_000,
                frames_sent: 40,
                frames_received: 300,
            }),
            UiDetailPanel::Layout(UiLayoutPanel {
                title: "Move this board's files?".to_string(),
                body: "The new firmware lays its files out differently.".to_string(),
                warning: None,
                download: board().child("disconnect"),
                continue_action: None,
                cancel: None,
            }),
            UiDetailPanel::OtherVersion {
                install: board().child("install-firmware"),
                from_file: Some(board().child("install-firmware-file")),
            },
            UiDetailPanel::RestoreFromFile {
                offer: board().child("restore-from-file"),
                current_base_mac: None,
            },
        ]
    }

    /// What `panel` shows that says it was drawn, and the path it presses.
    fn marker(panel: &UiDetailPanel) -> (&'static str, Option<OfferPath>) {
        match panel {
            UiDetailPanel::Terminal { .. } => ("3", None),
            UiDetailPanel::Access(_) => ("Access", None),
            UiDetailPanel::Bluetooth(_) => ("Restarting to turn Bluetooth on…", None),
            UiDetailPanel::Wifi(_) => ("Wi", None),
            UiDetailPanel::Rename { offer, .. } => ("Rename", Some(offer.clone())),
            UiDetailPanel::LinkCounters(_) => ("Link", None),
            UiDetailPanel::Layout(layout) => (
                "lays its files out differently",
                Some(layout.download.clone()),
            ),
            UiDetailPanel::OtherVersion { install, .. } => ("Other version", Some(install.clone())),
            UiDetailPanel::RestoreFromFile { offer, .. } => {
                ("Restore from a backup file", Some(offer.clone()))
            }
        }
    }

    /// The porch board's verbs, and stand-ins at the firmware panels' paths
    /// (any published action does: the panels read only the path, the
    /// label and the press).
    fn tree_with_firmware_verbs() -> UiOfferTree {
        let (_, mut tree) = card_and_tree(&porch_view());
        let forget = tree.get(&board().child("forget")).cloned().expect("forget");
        for (verb, label) in [
            ("install-firmware", "Other version…"),
            ("install-firmware-file", "From a file…"),
            ("restore-from-file", "Restore from a backup file…"),
        ] {
            tree.publish(UiOffer::new(
                board().child(verb),
                "download",
                forget.action.clone().with_label(label),
            ));
        }
        tree
    }

    fn render_panel(tree: UiOfferTree, panel: UiDetailPanel) -> String {
        render(PanelRoot, PanelRootProps { tree, panel })
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn PanelRoot(tree: UiOfferTree, panel: UiDetailPanel) -> Element {
        rsx! {
            OffersProvider { offers: tree,
                BarDetailPanel { panel, board: board(), on_action: |_| {} }
            }
        }
    }
}
