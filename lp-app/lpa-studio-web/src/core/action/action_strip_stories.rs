//! Stories for generic action rendering.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::core::ActionStrip;
use crate::core::story_fixtures::{disabled_action, lasting_action, story_actions};

#[story]
pub(crate) fn priorities() -> Element {
    rsx! {
        ActionStrip {
            actions: story_actions(),
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn disabled_reason() -> Element {
    rsx! {
        ActionStrip {
            actions: vec![disabled_action()],
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn running_state() -> Element {
    rsx! {
        ActionStrip {
            actions: story_actions(),
            running: true,
            on_action: move |_| {},
        }
    }
}

#[story(
    description = "A Lasting action (D7) in the strip: the error tint at rest, and the chip already as wide as its armed \"Confirm Erase\" reading — the first click arms, the second acts. There is no dialog."
)]
pub(crate) fn confirmation() -> Element {
    rsx! {
        ActionStrip {
            actions: vec![lasting_action()],
            running: false,
            on_action: move |_| {},
        }
    }
}
