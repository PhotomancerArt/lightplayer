use dioxus::prelude::*;
use lpa_studio_ux::{AvailableAction, StudioAction};

use crate::components::ActionButton;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionStrip(
    actions: Vec<AvailableAction<StudioAction>>,
    running: bool,
    on_action: EventHandler<StudioAction>,
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
