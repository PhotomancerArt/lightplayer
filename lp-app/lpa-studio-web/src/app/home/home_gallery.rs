//! The home gallery page: Devices / Projects / Examples.

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, RosterCardState, UiAction, UiHomeView, ZipBytes};

use crate::app::home::device_card::{
    ConnectDeviceCard, DeviceCard, connect_device_action, flash_device_action,
};
use crate::app::home::example_card::ExampleCard;
use crate::app::home::gallery_paste::{install_paste_listener, paste_from_clipboard};
use crate::app::home::package_card::{PackageCard, home_action};
use crate::base::{HelpLink, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, quiet_action_class};

/// The gallery home screen (roadmap M4, unconditional at `#/` since M5):
/// a map of everywhere the user's light lives. The runtime roster leads
/// the page (SDI addendum: Home reads window-switcher-first,
/// library-second); the connect card opens the VID-filtered chooser
/// directly — connecting is never a dialog trip (the old dialog's
/// `NeedsDevice` state is unreachable from here).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomeGallery(
    home: UiHomeView,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// Whether a serial device was ever granted (drives the Connected
    /// section collapse). `None` asks the browser-serial connector's
    /// granted-ports probe (lpa-link owns the `navigator.serial` FFI).
    #[props(default)]
    has_ever_granted: Option<bool>,
    /// Story-only override for the roster section's label, so the P4
    /// visual gate can compare the candidates ("Devices" / "Running" /
    /// "Open") from screenshots. Product code never passes it.
    #[props(default)]
    roster_label: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut drag_active = use_signal(|| 0_i32);
    // Cmd-V anywhere on the gallery installs a pasted project envelope.
    // The listener declines every paste that is not one — including
    // pastes aimed at a text field — so ordinary typing is untouched
    // (see `gallery_paste`).
    let _paste_listener = use_hook(move || install_paste_listener(on_action));
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
    // "Devices" holds until the P4 visual gate decides the label; the
    // override exists for the label-candidate stories only
    let roster_title = roster_label.unwrap_or_else(|| "Devices".to_string());
    let busy = home.opening.is_some();
    // Connected-EMPTY devices grow "Put on <name>" buttons on every
    // project card (state-flow model §1-A: the gallery IS the chooser
    // for a freshly set-up board; the target is always named).
    // (key, name): with two empty boards attached the NAME no longer
    // identifies the target (M4) — the key is what the push addresses.
    let empty_devices: Vec<(String, String)> = home
        .devices
        .iter()
        .filter(|card| !card.sim && matches!(card.state, RosterCardState::ConnectedEmpty))
        .map(|card| (card.identity_key().to_string(), card.name.clone()))
        .collect();
    let import_dropped = import_handler(on_action);
    let import_picked = import_dropped.clone();

    rsx! {
        div {
            class: "tw:relative tw:grid tw:content-start tw:gap-7",
            // drag-anywhere zip import (D2: files exist at the edges)
            ondragover: move |event| event.prevent_default(),
            ondragenter: move |event| {
                event.prevent_default();
                drag_active += 1;
            },
            ondragleave: move |_| drag_active -= 1,
            ondrop: move |event| {
                event.prevent_default();
                drag_active.set(0);
                import_dropped(event.files());
            },

            if let Some(issue) = home.issue.clone() {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-red-600/40 tw:bg-red-500/10 tw:px-4 tw:py-2.5 tw:text-sm tw:text-red-200",
                    span { "{issue.message}" }
                }
            }

            // --- The runtime roster (D27), TOP of the page (SDI addendum:
            // the spatial window switcher) — live sim + device sessions
            // and remembered devices; Projects follow below.
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
                        ConnectDeviceCard { on_action }
                    }
                }
            } else {
                div { class: "tw:flex tw:items-center tw:gap-2",
                    ActionButton {
                        action: connect_device_action(),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                    ActionButton {
                        action: flash_device_action(&flash_card_key, device_connected),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }

            // --- Projects ----------------------------------------------------
            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Projects" }
                    if home.library_available {
                        div { class: "tw:flex tw:items-center tw:gap-2",
                            // "New": create a pure-blank project and open it
                            // (2026-07-27 deviation from D17 — see the ADR at
                            // docs/adr/2026-07-27-node-authoring-operations.md)
                            ActionButton {
                                action: home_action(HomeOp::CreateProject),
                                running: busy,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                            // a real button (matching the ActionButton quiet
                            // chip exactly) that forwards to the hidden file
                            // input — a file dialog can't be a UiAction
                            button {
                                class: quiet_action_class(),
                                r#type: "button",
                                title: "Install a project from a zip archive.",
                                onclick: move |_| open_import_picker(),
                                span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                                    StudioIcon { name: StudioIconName::Upload, size: 14 }
                                }
                                span { "Import" }
                            }
                            input {
                                class: "tw:hidden",
                                id: "home-import-zip",
                                r#type: "file",
                                accept: ".zip",
                                onchange: move |event| import_picked(event.files()),
                            }
                            // Cmd-V anywhere on the page does this too (see
                            // `gallery_paste`); the button covers the cases
                            // where clipboard permission or focus does not
                            // deliver the event.
                            button {
                                class: quiet_action_class(),
                                r#type: "button",
                                title: "Install a project from a JSON envelope on the clipboard.",
                                onclick: move |_| paste_from_clipboard(on_action),
                                span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                                    StudioIcon { name: StudioIconName::Copy, size: 14 }
                                }
                                span { "Paste" }
                            }
                        }
                    }
                }
                if home.library_available {
                    if home.projects.is_empty() {
                        // create-first since the D17 deviation (2026-07-27):
                        // New makes a blank project; examples still seed a
                        // copy; imports arrive by button or drag
                        p { class: "tw:m-0 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:px-4 tw:py-5 tw:text-sm tw:text-muted-foreground",
                            "No projects yet — create a new project, or open an example below (it becomes yours on the first save). You can also drop a project zip anywhere on this page, or paste a project JSON envelope."
                        }
                    } else {
                        div { class: card_grid_class(),
                            for card in home.projects.clone() {
                                // opens arrive keyed by uid (menu paths)
                                // or slug (href navigation) — match either
                                PackageCard {
                                    key: "{card.uid}",
                                    opening: home.opening.as_deref() == Some(card.uid.as_str())
                                        || home.opening.as_deref() == Some(card.slug.as_str()),
                                    busy,
                                    card,
                                    now_secs,
                                    empty_devices: empty_devices.clone(),
                                    on_action,
                                }
                            }
                        }
                    }
                } else {
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                        "Local storage is unavailable, so there is no project library here. Examples still run."
                    }
                }
            }

            // --- Examples ---------------------------------------------------
            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-center tw:gap-3",
                    h2 { class: section_title_class(), "Examples" }
                    // kind filter chips: Modules stays hidden while no module
                    // examples exist (M6 grows this)
                    span { class: "tw:rounded-full tw:border tw:border-border tw:px-2.5 tw:py-0.5 tw:text-xs tw:font-semibold tw:text-muted-foreground",
                        "Projects"
                    }
                    // Where a WLED person goes looking for "the effects
                    // list" — the exact spot the shader question arises.
                    HelpLink {
                        href: crate::app::docs::docs_links::what_is_a_shader::HREF,
                        title: "What's a shader?",
                    }
                }
                div { class: card_grid_class(),
                    for card in home.examples.clone() {
                        ExampleCard {
                            key: "{card.id}",
                            opening: home.opening.as_deref() == Some(card.id.as_str()),
                            busy,
                            card,
                            on_action,
                        }
                    }
                }
            }

            if drag_active() > 0 {
                div { class: "tw:pointer-events-none tw:absolute tw:inset-0 tw:z-10 tw:grid tw:place-items-center tw:rounded-md tw:border-2 tw:border-dashed tw:border-accent tw:bg-background/80",
                    p { class: "tw:m-0 tw:text-base tw:font-semibold tw:text-strong-foreground",
                        "Drop a project zip, or paste a project JSON envelope"
                    }
                }
            }
        }
    }
}

/// Forward the Import button to the hidden file input (a file dialog
/// cannot be a `UiAction`; the button still wears the shared quiet chip).
#[cfg(target_arch = "wasm32")]
fn open_import_picker() {
    use wasm_bindgen::JsCast;
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("home-import-zip"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        input.click();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_import_picker() {}

/// Read every dropped/picked `.zip` and dispatch it as an import action.
fn import_handler(
    on_action: EventHandler<UiAction>,
) -> impl Fn(Vec<dioxus::html::FileData>) + Clone + 'static {
    move |files: Vec<dioxus::html::FileData>| {
        spawn(async move {
            for file in files {
                let name = file.name();
                if !name.to_lowercase().ends_with(".zip") {
                    log::warn!("import: skipping {name} (not a zip)");
                    continue;
                }
                match file.read_bytes().await {
                    Ok(bytes) => on_action.call(home_action(HomeOp::ImportZip {
                        file_name: name,
                        bytes: ZipBytes(bytes.to_vec()),
                    })),
                    Err(error) => log::warn!("import: could not read {name}: {error}"),
                }
            }
        });
    }
}

/// "Has a serial device ever been granted here?" for the Connected section
/// collapse — the browser-serial connector's catalog-level probe (the
/// `navigator.serial.getPorts()` FFI lives in lpa-link, not here).
#[cfg(target_arch = "wasm32")]
async fn probe_granted_serial_ports() -> bool {
    lpa_studio_core::BrowserSerialEsp32Provider::granted_ports_available().await
}

#[cfg(not(target_arch = "wasm32"))]
async fn probe_granted_serial_ports() -> bool {
    false
}

fn section_title_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:font-extrabold tw:uppercase tw:leading-none tw:text-heading"
}

fn card_grid_class() -> &'static str {
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(200px,1fr))] tw:gap-3.5"
}

/// The DEVICE roster grid (Yona hardware-walk feedback, width revised
/// down with the 2026-07-26 state-flow review): wide-enough columns and a
/// stretched row floor so the common operations — tab switches, opening a
/// sheet — resize the card WITHIN its footprint instead of reflowing the
/// page. Cards stretch to the row (grid default `align-items: stretch`),
/// so the min-height lives on the row, not the card component. Projects/
/// Examples keep the compact grid.
fn device_grid_class() -> &'static str {
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(260px,1fr))] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]"
}
