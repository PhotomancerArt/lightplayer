use dioxus::prelude::*;
use lpa_studio_core::UiStatus;
use lpa_studio_web_story_macros::story;

use crate::core::StatusChip;

#[story]
pub(crate) fn kinds() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Status kinds" }
            }
            div { class: "ux-actions",
                StatusChip { status: UiStatus::neutral("Choose connection") }
                StatusChip { status: UiStatus::working("Connecting") }
                StatusChip { status: UiStatus::good("Ready") }
                StatusChip { status: UiStatus::warning("Needs sync") }
                StatusChip { status: UiStatus::error("Failed") }
            }
        }
    }
}
