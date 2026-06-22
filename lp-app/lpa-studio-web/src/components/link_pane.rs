use dioxus::prelude::*;
use lpa_studio_ux::{LinkState, UxAction};

use crate::components::ActionStrip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LinkPane(
    state: LinkState,
    actions: Vec<UxAction>,
    running: bool,
    on_action: EventHandler<UxAction>,
) -> Element {
    let show_actions = !actions.is_empty()
        || matches!(
            state,
            LinkState::SelectingProvider { .. }
                | LinkState::SelectingEndpoint { .. }
                | LinkState::Failed { .. }
        );
    rsx! {
        section { class: "ux-panel ux-panel-primary",
            div { class: "ux-panel-heading",
                p { "Link" }
                h2 { "{link_title(&state)}" }
            }
            p { class: "ux-panel-copy", "{link_detail(&state)}" }
            if show_actions {
                ActionStrip {
                    actions,
                    running,
                    on_action,
                }
            }
        }
    }
}

fn link_title(state: &LinkState) -> String {
    match state {
        LinkState::SelectingProvider { .. } => "Choose runtime".to_string(),
        LinkState::DiscoveringEndpoints { .. } => "Discovering".to_string(),
        LinkState::SelectingEndpoint { .. } => "Choose endpoint".to_string(),
        LinkState::Connecting { .. } => "Connecting".to_string(),
        LinkState::Connected { device } => device.label.clone(),
        LinkState::Failed { .. } => "Link failed".to_string(),
    }
}

fn link_detail(state: &LinkState) -> String {
    match state {
        LinkState::SelectingProvider { providers } => providers
            .first()
            .map(|provider| provider.summary.clone())
            .unwrap_or_else(|| "No link providers are available.".to_string()),
        LinkState::DiscoveringEndpoints {
            provider_id,
            progress,
        } => progress
            .detail
            .clone()
            .unwrap_or_else(|| format!("Discovering endpoints from {}.", provider_id.label())),
        LinkState::SelectingEndpoint { endpoints, .. } => endpoints
            .first()
            .map(|endpoint| endpoint.summary.clone())
            .unwrap_or_else(|| "No endpoints are available for this provider.".to_string()),
        LinkState::Connecting { progress, .. } => progress
            .detail
            .clone()
            .unwrap_or_else(|| progress.label.clone()),
        LinkState::Connected { device } => {
            format!(
                "Connected through {} as {}.",
                device.provider_id, device.session_id
            )
        }
        LinkState::Failed { issue } => issue.message.clone(),
    }
}
