//! Stories for generic action rendering.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::story_fixtures::start_actions;
use crate::core::ActionStrip;

#[story]
pub(crate) fn provider_actions() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Provider actions" }
            }
            ActionStrip {
                actions: start_actions(),
                running: false,
                on_action: move |_| {},
            }
        }
    }
}
