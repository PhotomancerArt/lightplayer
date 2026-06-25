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
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-2",
            if actions.is_empty() {
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-muted-foreground", "No actions are currently available." }
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
