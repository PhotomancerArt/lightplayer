//! The Devices page (`#/`, vision D9): the runtime roster, the split
//! creation cards, and the setup wizard — the device half of the old
//! combined gallery, moved unchanged in the P09 page split.

use dioxus::prelude::*;
use lpa_studio_core::{RosterCardState, UiAction, UiHomeView};

use crate::app::home::device_card::{ConnectDeviceCard, DeviceCard, flash_device_action};
use crate::app::home::setup_wizard::SetupWizardCard;
use crate::app::home::{device_grid_class, section_title_class};
use crate::core::{ActionButton, ActionButtonVariant};

/// The device roster page (roadmap M4's gallery top, re-homed): a map of
/// everywhere the user's light runs. The runtime roster leads (SDI
/// addendum: window-switcher-first); the two entry cards open the setup
/// wizard, which asks for the port through the flow's own `RequestPort`
/// and lives in the roster in one of two frames (G2 ruling, 2026-08-05):
/// standalone card while nothing is attached, then the BODY of the bound
/// device's own card from the port grant on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DevicesPage(
    home: UiHomeView,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// Whether a serial device was ever granted (drives the roster
    /// collapse). `None` asks the browser-serial connector's
    /// granted-ports probe (lpa-link owns the `navigator.serial` FFI).
    #[props(default)]
    has_ever_granted: Option<bool>,
    /// Story-only override for the roster section's label (P4 visual
    /// gate comparisons). Product code never passes it.
    #[props(default)]
    roster_label: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // A finished device backup downloads exactly when its `seq` advances.
    // The view is a full snapshot, so without this paint key every
    // re-render would drop another copy of a megabyte-sized zip into the
    // user's Downloads folder (same discipline as the agent debug dump).
    let downloaded_backup_seq = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0_u64)));
    if let Some(backup) = &home.backup
        && downloaded_backup_seq.get() < backup.seq
    {
        downloaded_backup_seq.set(backup.seq);
        if let Err(error) =
            crate::app::home::package_export::trigger_zip_download(&backup.file_name, &backup.bytes)
        {
            log::warn!("device backup download failed: {error:?}");
        }
    }
    // only touch the browser's serial API when the caller didn't already
    // answer the grant question (stories always do — headless Chrome's
    // getPorts is crash-prone, and the probe is pointless there anyway)
    let probed_grant = use_resource(move || async move {
        match has_ever_granted {
            Some(granted) => granted,
            None => probe_granted_serial_ports().await,
        }
    });
    // the roster shows whenever it is non-empty or a grant exists
    let device_section_expanded =
        !home.devices.is_empty() || has_ever_granted.or(*probed_grant.read()).unwrap_or(false);
    // The roster HEADER's flash has no card behind it, so it can only
    // act directly when there is exactly ONE live board to mean (M4).
    // With two attached it cannot name one — and guessing is how the
    // wrong board gets flashed — so it falls back to the recovery
    // chooser, which asks. Per-board flashing lives on each card
    // (Set-up / Update / the Danger tab), which is where it belongs.
    // (The sim is not a device — D22 — and never a flash context.)
    let live_boards: Vec<&lpa_studio_core::UiDeviceCard> = home
        .devices
        .iter()
        .filter(|card| !card.sim && !matches!(card.state, RosterCardState::Offline { .. }))
        .collect();
    let flash_card_key = match live_boards.as_slice() {
        [only] => only.identity_key().to_string(),
        _ => String::new(),
    };
    let device_connected = live_boards.len() == 1;
    let roster_title = roster_label.unwrap_or_else(|| "Devices".to_string());

    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            if let Some(issue) = home.issue.clone() {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-red-600/40 tw:bg-red-500/10 tw:px-4 tw:py-2.5 tw:text-sm tw:text-red-200",
                    span { "{issue.message}" }
                }
            }

            // The runtime roster (D27) — live sim + device sessions and
            // remembered devices.
            if device_section_expanded {
                section { class: "tw:grid tw:gap-3",
                    header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                        h2 { class: section_title_class(), "{roster_title}" }
                        ActionButton {
                            action: flash_device_action(&flash_card_key, device_connected),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    div { class: device_grid_class(),
                        for card in home.devices.clone() {
                            DeviceCard {
                                // uid-based: device NAMES repeat (re-provisioned
                                // boards), and duplicate keys panic the diff
                                key: "{card.render_key()}",
                                sim: card.sim,
                                // The bound flow rides THIS card's body when
                                // the wizard names its key (G2 ruling): one
                                // physical board, one card, whose body is the
                                // wizard until the flow hands back.
                                setup: takeover_for(&home, &card),
                                card,
                                now_secs,
                                // M8′: the Project-tab picker's choices
                                // (empty-device cards offer the library)
                                project_choices: home
                                    .projects
                                    .iter()
                                    // A project this Studio cannot open is
                                    // not a thing to put on a board — it is
                                    // listed so it can be exported or
                                    // deleted, not deployed.
                                    .filter(|project| project.health.is_openable())
                                    .map(|project| lpa_studio_core::UiDeviceProjectChip {
                                        uid: project.uid.clone(),
                                        name: project.slug.clone(),
                                    })
                                    .collect::<Vec<_>>(),
                                on_action,
                            }
                        }
                        // The wizard in its STANDALONE frame: only while
                        // the flow has no card to be the body of (the
                        // pre-device states, and the sim path up to the
                        // start). Once it binds, it is up in the roster
                        // above riding the bound device's own card, and
                        // the entry card returns here — the flow moving
                        // between frames must never read as a card
                        // appearing or disappearing.
                        if let Some(wizard) = standalone_wizard(&home) {
                            SetupWizardCard { wizard, on_action }
                        } else {
                            ConnectDeviceCard { on_action }
                        }
                    }
                }
            } else if let Some(wizard) = standalone_wizard(&home) {
                // Nothing granted yet and no roster: the wizard still gets
                // its grid, because it IS the first card.
                div { class: device_grid_class(),
                    SetupWizardCard { wizard, on_action }
                }
            } else {
                // First run: no device has ever been granted here, so the
                // roster is empty — and the two entry cards ARE the page's
                // first move (device-first creation). The recovery flash
                // stays as a quiet chip beneath them.
                section { class: "tw:grid tw:gap-3",
                    div { class: device_grid_class(),
                        ConnectDeviceCard { on_action }
                    }
                    div { class: "tw:flex tw:items-center tw:gap-2",
                        ActionButton {
                            action: flash_device_action(&flash_card_key, device_connected),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The open flow when it has NO card to ride — the standalone entry-slot
/// wizard. Core names the card it binds to
/// ([`UiSetupWizard::takeover_card`](lpa_studio_core::UiSetupWizard)); a
/// named card means the wizard is already on the grid as that card's body,
/// so the entry slot goes back to being an entry card.
fn standalone_wizard(home: &UiHomeView) -> Option<lpa_studio_core::UiSetupWizard> {
    home.setup
        .clone()
        .filter(|wizard| wizard.takeover_card.is_none())
}

/// The open flow when it rides THIS card — matched on the card key core
/// resolved, never re-derived here (the key moves from session to uid
/// mid-flow, and two surfaces deriving it independently is how they drift).
fn takeover_for(
    home: &UiHomeView,
    card: &lpa_studio_core::UiDeviceCard,
) -> Option<lpa_studio_core::UiSetupWizard> {
    home.setup
        .clone()
        .filter(|wizard| wizard.takeover_card.as_deref() == Some(card.identity_key()))
}

/// "Has a serial device ever been granted here?" for the roster collapse
/// — the browser-serial connector's catalog-level probe (the
/// `navigator.serial.getPorts()` FFI lives in lpa-link, not here).
#[cfg(target_arch = "wasm32")]
async fn probe_granted_serial_ports() -> bool {
    lpa_studio_core::BrowserSerialEsp32Provider::granted_ports_available().await
}

#[cfg(not(target_arch = "wasm32"))]
async fn probe_granted_serial_ports() -> bool {
    false
}
