//! Device cards for the gallery's *Connected* section.
//!
//! Deliberately distinct from package cards (D12): a hardware header with a
//! connection dot and transport label instead of a thumbnail, and a parity
//! footer — a device must never read as "just another project".

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DeviceController, DeviceOp, LinkProviderKind, UiAction, UiDeviceCard,
    UiDeviceCardState,
};

use crate::app::home::time_ago::time_ago;
use crate::base::{StudioIcon, StudioIconName};

/// One known device. Clicking connects to it (via the browser's port
/// picker until devices carry stamped identities — M5 refines this).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceCard(
    card: UiDeviceCard,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(super::package_card::platform_now_secs);
    let (dot_class, status_line) = match &card.state {
        UiDeviceCardState::ConnectedRunning { project } => (
            "tw:h-2 tw:w-2 tw:rounded-full tw:bg-status-good-foreground",
            project
                .clone()
                .map(|project| format!("Running {project}"))
                .unwrap_or_else(|| "Connected".to_string()),
        ),
        UiDeviceCardState::RememberedOffline {
            last_seen_at,
            last_known,
        } => (
            "tw:h-2 tw:w-2 tw:rounded-full tw:bg-border-strong",
            match last_known {
                Some(project) => {
                    format!("Last ran {project} · seen {}", time_ago(now, *last_seen_at))
                }
                None => format!("Seen {}", time_ago(now, *last_seen_at)),
            },
        ),
    };
    let muted = matches!(card.state, UiDeviceCardState::RememberedOffline { .. });

    rsx! {
        article {
            class: device_card_class(muted),
            onclick: move |_| on_action.call(connect_device_action()),
            header { class: "tw:flex tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:px-3 tw:py-2",
                span { class: dot_class }
                span { class: "tw:inline-flex tw:items-center tw:text-muted-foreground",
                    StudioIcon { name: StudioIconName::Usb, size: 14 }
                }
                span { class: "tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-muted-foreground",
                    "{card.transport}"
                }
            }
            div { class: "tw:grid tw:gap-0.5 tw:p-3",
                p { class: device_name_class(muted), "{card.name}" }
                p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{status_line}" }
            }
        }
    }
}

/// The dashed "Connect a device" affordance card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        button {
            class: "tw:grid tw:min-h-24 tw:cursor-pointer tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:p-3 tw:text-muted-foreground tw:transition-colors tw:hover:border-accent tw:hover:text-strong-foreground",
            r#type: "button",
            onclick: move |_| on_action.call(connect_device_action()),
            span { class: "tw:inline-flex tw:items-center tw:gap-2",
                StudioIcon { name: StudioIconName::Usb, size: 16 }
                span { class: "tw:text-sm tw:font-semibold", "Connect a device" }
            }
            span { class: "tw:text-xs tw:text-dim-foreground", "ESP32 over USB" }
        }
    }
}

/// Connect = the browser serial flow (the classic device pane takes over —
/// the M4 bridge; M5 replaces it with the provision dialog).
pub(crate) fn connect_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::OpenProvider {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        },
    )
}

/// The "Flash firmware…" bridge link action (open without attaching).
pub(crate) fn flash_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::OpenProviderForRecovery {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        },
    )
}

fn device_card_class(muted: bool) -> &'static str {
    if muted {
        "tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-70 tw:transition-opacity tw:hover:opacity-100"
    } else {
        "tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
    }
}

fn device_name_class(muted: bool) -> &'static str {
    if muted {
        "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-muted-foreground"
    } else {
        "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground"
    }
}
