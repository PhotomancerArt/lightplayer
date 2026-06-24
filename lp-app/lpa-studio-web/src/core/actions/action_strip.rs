use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use crate::core::ActionButton;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionStrip(
    actions: Vec<UiAction>,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "ux-actions",
            if actions.is_empty() {
                p { class: "ux-panel-copy", "No actions are currently available." }
            } else {
                for action in actions {
                    ActionButton {
                        action,
                        running,
                        on_action,
                    }
                }
            }
        }
    }
}
