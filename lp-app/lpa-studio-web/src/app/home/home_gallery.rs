//! The home gallery page: Connected / Your projects / Examples.

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction, UiHomeView, ZipBytes};

use crate::app::home::device_card::{
    ConnectDeviceCard, DeviceCard, connect_device_action, flash_device_action,
};
use crate::app::home::example_card::ExampleCard;
use crate::app::home::package_card::{PackageCard, home_action};
use crate::base::{StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, quiet_action_class};

/// The gallery home screen (roadmap M4, unconditional at `#/` since M5):
/// a map of everywhere the user's light lives. The connect card and the
/// flash link open the deploy dialog — connect, provision, and push all
/// live there (D22/D24).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HomeGallery(
    home: UiHomeView,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// Whether a serial device was ever granted (drives the Connected
    /// section collapse). `None` probes `navigator.serial.getPorts()`.
    #[props(default)]
    has_ever_granted: Option<bool>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut drag_active = use_signal(|| 0_i32);
    // only touch the browser's serial API when the caller didn't already
    // answer the grant question (stories always do — headless Chrome's
    // getPorts is crash-prone, and the probe is pointless there anyway)
    let probed_grant = use_resource(move || async move {
        match has_ever_granted {
            Some(granted) => granted,
            None => probe_granted_serial_ports().await > 0,
        }
    });
    let device_section_expanded =
        !home.devices.is_empty() || has_ever_granted.or(*probed_grant.read()).unwrap_or(false);
    let busy = home.opening.is_some();
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

            // --- Connected ------------------------------------------------
            if device_section_expanded {
                section { class: "tw:grid tw:gap-3",
                    header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                        h2 { class: section_title_class(), "Connected" }
                        ActionButton {
                            action: flash_device_action(),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    div { class: card_grid_class(),
                        for card in home.devices.clone() {
                            DeviceCard {
                                key: "{card.name}",
                                card,
                                now_secs,
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
                        action: flash_device_action(),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }

            // --- Your projects ---------------------------------------------
            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Your projects" }
                    if home.library_available {
                        div { class: "tw:flex tw:items-center tw:gap-2",
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
                        }
                    }
                }
                if home.library_available {
                    if home.projects.is_empty() {
                        // the one way to start: open an example (D17 — "new
                        // project" IS the examples place); imports arrive by
                        // button or drag
                        p { class: "tw:m-0 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:px-4 tw:py-5 tw:text-sm tw:text-muted-foreground",
                            "No projects yet — open an example below to start. It becomes your project on the first save. You can also drop a project zip anywhere on this page."
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
                        "Drop a project zip to import it"
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

/// `navigator.serial.getPorts()` count via reflection (no `web_sys::Serial`
/// feature plumbing): "has a device ever been granted" for the Connected
/// section collapse.
#[cfg(target_arch = "wasm32")]
async fn probe_granted_serial_ports() -> usize {
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else {
        return 0;
    };
    let navigator = window.navigator();
    let Ok(serial) = js_sys::Reflect::get(&navigator, &"serial".into()) else {
        return 0;
    };
    if serial.is_undefined() || serial.is_null() {
        return 0;
    }
    let Ok(get_ports) = js_sys::Reflect::get(&serial, &"getPorts".into()) else {
        return 0;
    };
    let Ok(get_ports) = get_ports.dyn_into::<js_sys::Function>() else {
        return 0;
    };
    let Ok(promise) = get_ports.call0(&serial) else {
        return 0;
    };
    let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else {
        return 0;
    };
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(ports) => js_sys::Array::from(&ports).length() as usize,
        Err(_) => 0,
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn probe_granted_serial_ports() -> usize {
    0
}

fn section_title_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:font-extrabold tw:uppercase tw:leading-none tw:text-heading"
}

fn card_grid_class() -> &'static str {
    "tw:grid tw:grid-cols-[repeat(auto-fill,minmax(200px,1fr))] tw:gap-3.5"
}
