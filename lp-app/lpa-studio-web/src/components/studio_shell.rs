use dioxus::prelude::*;
use lpa_studio_ux::{StudioView, UiAction};

use crate::components::{RuntimeLog, UxPane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(view: StudioView, running: bool, on_action: EventHandler<UiAction>) -> Element {
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
