use dioxus::prelude::*;
use lp_studio_core::StudioState;

#[component]
pub fn ProjectPanel(state: StudioState) -> Element {
    let project_id = state
        .project_session
        .as_ref()
        .map(|project| project.project_id.clone())
        .unwrap_or_else(|| "not loaded".to_string());
    let handle = state
        .project_session
        .as_ref()
        .map(|project| project.handle.id().to_string())
        .unwrap_or_else(|| "-".to_string());
    let selected = state
        .project_session
        .as_ref()
        .and_then(|project| project.selected_node_id.clone())
        .unwrap_or_else(|| "none".to_string());
    rsx! {
        section { class: "panel",
            div { class: "panel-heading",
                h2 { "Project" }
            }
            dl {
                dt { "Project" }
                dd { "{project_id}" }
                dt { "Handle" }
                dd { "{handle}" }
                dt { "Selection" }
                dd { "{selected}" }
            }
        }
    }
}
