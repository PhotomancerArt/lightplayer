use dioxus::prelude::*;
use lpa_studio_ux::ServerState;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ServerPane(state: ServerState) -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                p { "Server" }
                h2 { "{server_title(&state)}" }
            }
            p { class: "ux-panel-copy", "{server_detail(&state)}" }
        }
    }
}

fn server_title(state: &ServerState) -> &'static str {
    match state {
        ServerState::Disconnected => "Offline",
        ServerState::Connecting { .. } => "Connecting",
        ServerState::Connected { .. } => "Connected",
        ServerState::Failed { .. } => "Failed",
    }
}

fn server_detail(state: &ServerState) -> String {
    match state {
        ServerState::Disconnected => {
            "Open a link endpoint to attach the server protocol.".to_string()
        }
        ServerState::Connecting { progress } => progress.label.clone(),
        ServerState::Connected { protocol } => format!("Protocol: {protocol}"),
        ServerState::Failed { issue } => issue.message.clone(),
    }
}
