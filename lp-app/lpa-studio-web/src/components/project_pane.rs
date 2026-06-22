use dioxus::prelude::*;
use lpa_studio_ux::{ProjectState, UxAction};

use crate::components::ActionStrip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectPane(
    state: ProjectState,
    actions: Vec<UxAction>,
    running: bool,
    on_action: EventHandler<UxAction>,
) -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                p { "Project" }
                h2 { "{project_title(&state)}" }
            }
            ProjectDetails { state }
            if !actions.is_empty() {
                ActionStrip {
                    actions,
                    running,
                    on_action,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectDetails(state: ProjectState) -> Element {
    match state {
        ProjectState::Ready {
            project_id,
            handle_id,
            inventory,
        } => rsx! {
            dl { class: "ux-metrics",
                div {
                    dt { "Project" }
                    dd { "{project_id}" }
                }
                div {
                    dt { "Handle" }
                    dd { "{handle_id}" }
                }
                div {
                    dt { "Nodes" }
                    dd { "{inventory.node_count}" }
                }
                div {
                    dt { "Definitions" }
                    dd { "{inventory.definition_count}" }
                }
                div {
                    dt { "Assets" }
                    dd { "{inventory.asset_count}" }
                }
            }
        },
        other => rsx! {
            p { class: "ux-panel-copy", "{project_detail(&other)}" }
        },
    }
}

fn project_title(state: &ProjectState) -> &'static str {
    match state {
        ProjectState::NotLoaded => "Not loaded",
        ProjectState::LoadingDemoProject { .. } => "Loading",
        ProjectState::Ready { .. } => "Ready",
        ProjectState::Failed { .. } => "Failed",
    }
}

fn project_detail(state: &ProjectState) -> String {
    match state {
        ProjectState::NotLoaded => {
            "Load the demo project after the simulator is connected.".to_string()
        }
        ProjectState::LoadingDemoProject { progress } => progress.label.clone(),
        ProjectState::Ready { .. } => "Project inventory is ready.".to_string(),
        ProjectState::Failed { issue } => issue.message.clone(),
    }
}
