use dioxus::prelude::*;
use lpa_studio_ux::{StudioView, UiAction};

use crate::components::{RuntimeLog, UxPane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    view: StudioView,
    running: bool,
    error: Option<String>,
    notices: Vec<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let has_error = error.is_some();
    let StudioView { panes, logs } = view;
    let layout_class = match panes.len() {
        0 | 1 => "ux-layout ux-layout-single",
        2 => "ux-layout ux-layout-split",
        _ => "ux-layout ux-layout-triple",
    };

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

            section { class: "{layout_class}",
                for (index, pane) in panes.into_iter().enumerate() {
                    UxPane {
                        key: "{pane.node_id}",
                        view: pane,
                        primary: index == 0,
                        running,
                        on_action,
                    }
                }
            }

            RuntimeLog { logs }
        }
    }
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
