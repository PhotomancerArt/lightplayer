use dioxus::prelude::*;
use lpa_studio_core::UiStatus;

use crate::core::StatusChip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaneFrame(
    title: String,
    primary: bool,
    status: Option<UiStatus>,
    children: Element,
) -> Element {
    let panel_class = if primary {
        "ux-panel ux-panel-primary"
    } else {
        "ux-panel"
    };

    rsx! {
        section { class: "{panel_class}",
            div { class: "ux-panel-heading",
                p { "{title}" }
                if let Some(status) = status {
                    StatusChip { status }
                }
            }
            {children}
        }
    }
}
