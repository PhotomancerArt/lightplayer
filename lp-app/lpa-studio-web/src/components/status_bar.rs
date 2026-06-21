use dioxus::prelude::*;
use lpa_studio_core::StudioState;

#[component]
pub fn StatusBar(state: StudioState, running: bool, error: Option<String>) -> Element {
    let status = if running {
        "Starting"
    } else if state.project_session.is_some() {
        "Ready"
    } else if state.client_session.is_some() {
        "Connected"
    } else {
        "Idle"
    };
    let heartbeat = state
        .heartbeat
        .as_ref()
        .map(|heartbeat| {
            format!(
                "{:.0} fps | frame {}",
                heartbeat.fps_avg, heartbeat.frame_count
            )
        })
        .unwrap_or_else(|| "waiting for heartbeat".to_string());
    rsx! {
        header { class: "status-bar",
            div {
                h1 { "LightPlayer Studio" }
                p { "Firmware runtime and hardware control" }
            }
            div { class: "status-pill", "{status}" }
            div { class: "status-metric", "{heartbeat}" }
            if let Some(error) = error {
                div { class: "status-error", "{error}" }
            }
        }
    }
}
