use dioxus::prelude::*;
use lp_studio_core::{DeviceAccessStatus, StudioState};

#[component]
pub fn DevicePanel(
    state: StudioState,
    running: bool,
    on_start_demo: EventHandler<MouseEvent>,
    on_connect_hardware: EventHandler<MouseEvent>,
) -> Element {
    let provider = state
        .device_manager
        .providers
        .selected_provider_id()
        .map(|provider_id| provider_id.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    let endpoint_count = state
        .device_manager
        .providers
        .selected_provider_endpoints()
        .len();
    let access = state
        .device_access
        .as_ref()
        .map(|access| access_status_label(&access.status))
        .unwrap_or_else(|| "not requested".to_string());
    let session = state
        .device_session
        .as_ref()
        .map(|session| session.session_id.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    rsx! {
        section { class: "panel device-panel",
            div { class: "panel-heading",
                h2 { "Device" }
                div { class: "button-row",
                    button {
                        disabled: running,
                        onclick: move |event| on_start_demo.call(event),
                        if running { "Running" } else { "Start local" }
                    }
                    button {
                        disabled: running,
                        onclick: move |event| on_connect_hardware.call(event),
                        if running { "Running" } else { "Connect hardware" }
                    }
                }
            }
            dl {
                dt { "Provider" }
                dd { "{provider}" }
                dt { "Access" }
                dd { "{access}" }
                dt { "Endpoints" }
                dd { "{endpoint_count}" }
                dt { "Session" }
                dd { "{session}" }
            }
        }
    }
}

fn access_status_label(status: &DeviceAccessStatus) -> String {
    match status {
        DeviceAccessStatus::Unknown => "unknown".to_string(),
        DeviceAccessStatus::Unsupported { reason } => format!("unsupported: {reason}"),
        DeviceAccessStatus::PermissionRequired => "permission required".to_string(),
        DeviceAccessStatus::PermissionDenied { reason } => format!("denied: {reason}"),
        DeviceAccessStatus::Granted => "granted".to_string(),
    }
}
