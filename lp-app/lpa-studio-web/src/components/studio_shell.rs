use dioxus::prelude::*;
use lpa_studio_ux::{LinkUx, ProjectUx, StudioSnapshot, UxAction};

use crate::components::{LinkPane, ProjectPane, RuntimeLog, ServerPane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    snapshot: StudioSnapshot,
    actions: Vec<UxAction>,
    running: bool,
    error: Option<String>,
    notices: Vec<String>,
    on_action: EventHandler<UxAction>,
) -> Element {
    let has_error = error.is_some();
    let link_actions = actions_for_node(&actions, LinkUx::NODE_ID);
    let project_actions = actions_for_node(&actions, ProjectUx::NODE_ID);

    rsx! {
        main { class: "ux-shell",
            header { class: "ux-header",
                div {
                    p { class: "ux-eyebrow", "LightPlayer Studio" }
                }
                div { class: status_class(running, has_error),
                    if running {
                        "Running"
                    } else if error.is_some() {
                        "Needs attention"
                    } else {
                        "Ready"
                    }
                }
            }

            if let Some(message) = error.as_ref() {
                section { class: "ux-alert ux-alert-error",
                    strong { "Action failed" }
                    p { "{message}" }
                }
            }

            if !notices.is_empty() {
                section { class: "ux-notices",
                    for notice in notices.iter() {
                        p { "{notice}" }
                    }
                }
            }

            section { class: "ux-layout",
                LinkPane {
                    state: snapshot.link.state,
                    actions: link_actions,
                    running,
                    on_action,
                }
                ServerPane { state: snapshot.server.state }
                ProjectPane {
                    state: snapshot.project.state,
                    actions: project_actions,
                    running,
                    on_action,
                }
            }

            RuntimeLog { logs: snapshot.logs }
        }
    }
}

fn actions_for_node(actions: &[UxAction], node_id: &str) -> Vec<UxAction> {
    actions
        .iter()
        .filter(|action| action.is_for_node(node_id))
        .cloned()
        .collect()
}

fn status_class(running: bool, has_error: bool) -> &'static str {
    if has_error {
        "ux-status ux-status-error"
    } else if running {
        "ux-status ux-status-running"
    } else {
        "ux-status"
    }
}
