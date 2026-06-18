use dioxus::prelude::*;
use lp_studio_core::StudioState;

#[component]
pub fn DevicePanel(
    state: StudioState,
    running: bool,
    on_start_demo: EventHandler<MouseEvent>,
) -> Element {
    let provider = state
        .link_selection
        .selected_provider_id
        .as_str()
        .to_string();
    let endpoint_count = state.link_selection.endpoints.len();
    let session = state
        .device_session
        .as_ref()
        .map(|session| session.session_id.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    rsx! {
        section { class: "panel device-panel",
            div { class: "panel-heading",
                h2 { "Device" }
                button {
                    disabled: running,
                    onclick: move |event| on_start_demo.call(event),
                    if running { "Running" } else { "Start demo" }
                }
            }
            dl {
                dt { "Provider" }
                dd { "{provider}" }
                dt { "Endpoints" }
                dd { "{endpoint_count}" }
                dt { "Session" }
                dd { "{session}" }
            }
        }
    }
}
