//! Stories for generic action rendering.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::core::ActionStrip;
use crate::core::story_fixtures::{confirmation_action, disabled_action, story_actions};

#[story]
pub(crate) fn priorities() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Action priorities" }
            }
            ActionStrip {
                actions: story_actions(),
                running: false,
                on_action: move |_| {},
            }
        }
    }
}

#[story]
pub(crate) fn disabled_reason() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Disabled action" }
            }
            ActionStrip {
                actions: vec![disabled_action()],
                running: false,
                on_action: move |_| {},
            }
        }
    }
}

#[story]
pub(crate) fn running_state() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Running action strip" }
            }
            ActionStrip {
                actions: story_actions(),
                running: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story]
pub(crate) fn confirmation() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Confirmation action" }
            }
            ActionStrip {
                actions: vec![confirmation_action()],
                running: false,
                on_action: move |_| {},
            }
        }
    }
}
