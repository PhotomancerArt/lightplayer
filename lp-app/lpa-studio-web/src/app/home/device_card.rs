//! Device cards for the gallery's *Connected* section.
//!
//! Deliberately distinct from package cards (D12): a hardware header with a
//! connection dot and transport label instead of a thumbnail, and a parity
//! footer — a device must never read as "just another project".

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceController, DeviceOp, UiAction, UiDeviceCard,
    UiDeviceCardState,
};

use crate::app::home::time_ago::time_ago;
use crate::base::{StudioIcon, StudioIconName};

/// One known device. Clicking an offline/remembered card reconnects
/// through an already-granted serial port with no chooser (M1); other
/// states open the deploy dialog.
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
        UiDeviceCardState::Blank => (
            "tw:h-2 tw:w-2 tw:rounded-full tw:bg-status-warning-foreground",
            "Ready to set up — install firmware".to_string(),
        ),
        UiDeviceCardState::ConnectedRunning { project } => (
            "tw:h-2 tw:w-2 tw:rounded-full tw:bg-status-good-foreground",
            project
                .clone()
                .map(|project| format!("Running {project}"))
                .unwrap_or_else(|| "Connected".to_string()),
        ),
        UiDeviceCardState::ConnectedUnknown { detail } => (
            "tw:h-2 tw:w-2 tw:rounded-full tw:bg-status-good-foreground",
            detail.clone(),
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
    // offline/remembered → one-click reconnect over a granted port (M1);
    // everything else keeps the deploy-dialog entry
    let click_action = if muted {
        reconnect_device_action()
    } else {
        connect_device_action()
    };

    rsx! {
        article {
            class: device_card_class(muted),
            // every device card is a drop target: project card → device
            // card opens the deploy dialog pre-filled (replace preview)
            ondragover: move |event| event.prevent_default(),
            ondrop: move |event| {
                event.prevent_default();
                if let Some(key) = super::package_card::take_dragged_project() {
                    on_action.call(UiAction::from_op(
                        ControllerId::new(DEPLOY_NODE_ID),
                        DeployOp::OpenDialog { target_key: Some(key) },
                    ));
                }
            },
            onclick: move |_| on_action.call(click_action.clone()),
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

/// The dashed "Connect a device" affordance card. Copy comes from the
/// action's own metadata so the card and the toolbar chip never drift.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    let action = connect_device_action();
    let meta = action.meta().clone();
    rsx! {
        button {
            class: "tw:grid tw:min-h-24 tw:cursor-pointer tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:p-3 tw:text-muted-foreground tw:transition-colors tw:hover:border-accent tw:hover:text-strong-foreground",
            r#type: "button",
            title: "{meta.summary}",
            onclick: move |_| on_action.call(action.clone()),
            span { class: "tw:inline-flex tw:items-center tw:gap-2",
                StudioIcon { name: StudioIconName::Usb, size: 16 }
                span { class: "tw:text-sm tw:font-semibold", "{meta.label}" }
            }
            span { class: "tw:text-xs tw:text-dim-foreground", "{meta.summary}" }
        }
    }
}

/// One-click reconnect for an offline/remembered device (M1): connect a
/// granted serial port directly; the browser chooser only appears when no
/// grant exists.
pub(crate) fn reconnect_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::ReconnectDevice,
    )
}

/// Connect = open the deploy dialog (M5): connect, provision, and push
/// all live there; the gallery never becomes a pane takeover.
pub(crate) fn connect_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::OpenDialog { target_key: None },
    )
    .with_label("Connect a device")
    .with_summary("Connect, provision, and push to a device.")
    .with_icon("usb")
}

/// "Flash firmware…" also enters the dialog — its blank/needs-device
/// states carry the flash and recovery flows.
pub(crate) fn flash_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::OpenDialog { target_key: None },
    )
    .with_label("Flash firmware…")
    .with_summary("Install or repair LightPlayer firmware on an ESP32.")
    .with_icon("zap")
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
